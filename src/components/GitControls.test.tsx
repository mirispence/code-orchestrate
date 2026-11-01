import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, vi } from "vitest";

import { GitControls } from "./GitControls";
import { GitCommandsContext, useGitCommands } from "@/lib/useGitCommands";
import { api, type GitCommandError, type GitStatus, type Result, type Workspace } from "@/lib/api";

vi.mock("@/lib/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/api")>();
  return { ...actual, api: { gitStatus: vi.fn(), runGitCommand: vi.fn(), runGitInTerminal: vi.fn() } };
});

const mocked = vi.mocked(api);

const ws: Workspace = {
  id: 7,
  name: "app",
  path: "E:\\src\\app",
  pinned: false,
  note: null,
  createdAt: 0,
  tags: [],
  formerPaths: [],
  missing: false,
};

function gitStatus(overrides: Partial<GitStatus> = {}): GitStatus {
  return {
    branch: "feature",
    tracking: { ahead: 2, behind: 3 },
    changes: false,
    remote: "origin",
    defaultBranch: "main",
    ...overrides,
  };
}

/** As the app has it: the commands held above a view that comes and goes. */
function Controls({ workspace, onFoundMissing, shown = true }: { workspace: Workspace; onFoundMissing: () => void; shown?: boolean }) {
  return (
    <GitCommandsContext.Provider value={useGitCommands()}>
      {shown && <GitControls workspace={workspace} onFoundMissing={onFoundMissing} />}
    </GitCommandsContext.Provider>
  );
}

function renderControls(workspace = ws) {
  const onFoundMissing = vi.fn();
  const view = render(<Controls workspace={workspace} onFoundMissing={onFoundMissing} />);
  return { onFoundMissing, ...view };
}

const button = (name: string | RegExp) => screen.findByRole("button", { name });

function failure(message: string, kind: "failed" | "uncommittedChanges" = "failed"): Result<null, GitCommandError> {
  const error = kind === "failed" ? { kind, message, output: message } : { kind, message, branch: "main" };
  return { ok: false, error };
}

function refusal(error: GitCommandError): Result<null, GitCommandError> {
  return { ok: false, error };
}

/** A promise settled by hand, to catch something mid-flight. */
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((r) => (resolve = r));
  return { promise, resolve };
}

beforeEach(() => {
  vi.resetAllMocks();
  mocked.gitStatus.mockResolvedValue(gitStatus());
  mocked.runGitCommand.mockResolvedValue({ ok: true, value: null });
  mocked.runGitInTerminal.mockResolvedValue({ ok: true, value: null });
});

describe("The git status line", () => {
  it("shows the branch and ahead/behind counts", async () => {
    renderControls();
    expect(await screen.findByText("feature")).toBeInTheDocument();
    expect(screen.getByLabelText("2 ahead, 3 behind")).toBeInTheDocument();
    expect(screen.queryByText("Uncommitted changes")).not.toBeInTheDocument();
  });

  it("shows uncommitted changes", async () => {
    mocked.gitStatus.mockResolvedValue(gitStatus({ changes: true }));
    renderControls();
    expect(await screen.findByText("Uncommitted changes")).toBeInTheDocument();
  });

  it("handles a detached HEAD and a branch with no upstream", async () => {
    mocked.gitStatus.mockResolvedValue(gitStatus({ branch: null, tracking: null }));
    const { unmount } = renderControls();
    expect(await screen.findByText("Detached HEAD")).toBeInTheDocument();
    unmount();

    mocked.gitStatus.mockResolvedValue(gitStatus({ tracking: null }));
    renderControls();
    expect(await screen.findByText("No upstream")).toBeInTheDocument();
  });

  it("is hidden outside a work tree", async () => {
    mocked.gitStatus.mockResolvedValue(null);
    renderControls();
    await waitFor(() => expect(mocked.gitStatus).toHaveBeenCalled());
    expect(screen.queryByRole("button", { name: "Pull" })).not.toBeInTheDocument();
  });

  it("is hidden for a Missing Workspace, without asking git", async () => {
    renderControls({ ...ws, missing: true });
    await act(async () => {});
    expect(mocked.gitStatus).not.toHaveBeenCalled();
    expect(screen.queryByRole("button", { name: "Pull" })).not.toBeInTheDocument();
  });
});

describe("Refreshing the status", () => {
  it("rereads on every window focus, even right after the last read", async () => {
    renderControls();
    await button("Pull");
    mocked.gitStatus.mockResolvedValue(gitStatus({ branch: "other" }));

    act(() => void window.dispatchEvent(new Event("focus")));

    expect(await screen.findByText("other")).toBeInTheDocument();
    expect(mocked.gitStatus).toHaveBeenCalledTimes(2);
  });

  it("does one more read after a focus that arrives during a read", async () => {
    const first = deferred<GitStatus | null>();
    mocked.gitStatus.mockReturnValueOnce(first.promise);
    renderControls();

    act(() => void window.dispatchEvent(new Event("focus")));
    act(() => void window.dispatchEvent(new Event("focus")));
    expect(mocked.gitStatus).toHaveBeenCalledTimes(1);

    mocked.gitStatus.mockResolvedValue(gitStatus({ branch: "after" }));
    await act(async () => first.resolve(gitStatus()));

    expect(await screen.findByText("after")).toBeInTheDocument();
    expect(mocked.gitStatus).toHaveBeenCalledTimes(2);
  });

  it("rereads on the manual refresh button", async () => {
    const user = userEvent.setup();
    renderControls();
    await user.click(await button("Refresh git status"));
    await waitFor(() => expect(mocked.gitStatus).toHaveBeenCalledTimes(2));
  });

  it("rereads after a command finishes", async () => {
    const user = userEvent.setup();
    renderControls();
    await user.click(await button("Pull"));
    await waitFor(() => expect(mocked.gitStatus).toHaveBeenCalledTimes(2));
  });
});

describe("Which commands are available", () => {
  it("runs each command in this Workspace", async () => {
    const user = userEvent.setup();
    renderControls();
    await user.click(await button("Pull"));
    await waitFor(() => expect(mocked.runGitCommand).toHaveBeenCalledWith(7, "pull"));
    await waitFor(async () => expect(await button("Push")).toBeEnabled());
    await user.click(await button("Push"));
    await waitFor(() => expect(mocked.runGitCommand).toHaveBeenCalledWith(7, "push"));
    await waitFor(async () => expect(await button("Checkout main")).toBeEnabled());
    await user.click(await button("Checkout main"));
    await waitFor(() => expect(mocked.runGitCommand).toHaveBeenCalledWith(7, "checkoutDefault"));
  });

  it("disables Pull and Push with a tooltip when there is no remote, keeping Checkout", async () => {
    mocked.gitStatus.mockResolvedValue(gitStatus({ remote: null }));
    renderControls();
    const pull = await button("Pull");
    expect(pull).toBeDisabled();
    expect(pull.parentElement).toHaveAttribute("title", expect.stringContaining("no remote"));
    expect(await button("Push")).toBeDisabled();
    expect(await button("Checkout main")).toBeEnabled();
  });

  it("disables Push on a detached HEAD", async () => {
    mocked.gitStatus.mockResolvedValue(gitStatus({ branch: null, tracking: null }));
    renderControls();
    expect(await button("Push")).toBeDisabled();
    expect(await button("Pull")).toBeEnabled();
  });

  it("disables Checkout when no default branch is found or it is already checked out", async () => {
    mocked.gitStatus.mockResolvedValue(gitStatus({ defaultBranch: null }));
    const { unmount } = renderControls();
    expect(await button(/^Checkout/)).toBeDisabled();
    unmount();

    mocked.gitStatus.mockResolvedValue(gitStatus({ branch: "main" }));
    renderControls();
    expect(await button("Checkout main")).toBeDisabled();
  });

  it("leaves Checkout enabled with uncommitted changes: the backend refuses and says why", async () => {
    const user = userEvent.setup();
    mocked.gitStatus.mockResolvedValue(gitStatus({ changes: true }));
    mocked.runGitCommand.mockResolvedValue(failure("There are uncommitted changes", "uncommittedChanges"));
    renderControls();

    await user.click(await button("Checkout main"));

    expect(await screen.findByRole("alert")).toHaveTextContent("There are uncommitted changes");
    expect(screen.queryByRole("button", { name: "Run in terminal" })).not.toBeInTheDocument();
  });
});

describe("While a command runs", () => {
  it("disables this Workspace's buttons and marks the running one", async () => {
    const user = userEvent.setup();
    const finish = deferred<Result<null, GitCommandError>>();
    mocked.runGitCommand.mockReturnValue(finish.promise);
    renderControls();

    await user.click(await button("Pull"));

    expect(await button("Pull")).toBeDisabled();
    expect(await button("Pull")).toHaveAttribute("aria-busy", "true");
    expect(await button("Push")).toBeDisabled();
    expect(await button("Checkout main")).toBeDisabled();

    await act(async () => finish.resolve({ ok: true, value: null }));
    await waitFor(async () => expect(await button("Pull")).toBeEnabled());
  });

  it("leaves another Workspace usable", async () => {
    const user = userEvent.setup();
    mocked.runGitCommand.mockReturnValue(deferred<Result<null, GitCommandError>>().promise);
    const onFoundMissing = vi.fn();
    const { rerender } = render(<Controls workspace={ws} onFoundMissing={onFoundMissing} />);
    await user.click(await button("Pull"));

    rerender(<Controls workspace={{ ...ws, id: 8 }} onFoundMissing={onFoundMissing} />);

    await waitFor(async () => expect(await button("Pull")).toBeEnabled());
    rerender(<Controls workspace={ws} onFoundMissing={onFoundMissing} />);
    await waitFor(async () => expect(await button("Pull")).toBeDisabled());
  });

  it("is still shown after leaving for Settings and coming back", async () => {
    const user = userEvent.setup();
    mocked.runGitCommand.mockReturnValue(deferred<Result<null, GitCommandError>>().promise);
    const onFoundMissing = vi.fn();
    const { rerender } = render(<Controls workspace={ws} onFoundMissing={onFoundMissing} />);
    await user.click(await button("Pull"));

    rerender(<Controls workspace={ws} onFoundMissing={onFoundMissing} shown={false} />);
    rerender(<Controls workspace={ws} onFoundMissing={onFoundMissing} />);

    expect(await button("Pull")).toHaveAttribute("aria-busy", "true");
  });
});

describe("A failed command", () => {
  it("shows git's error output and Run in terminal for a failed Pull", async () => {
    const user = userEvent.setup();
    mocked.runGitCommand.mockResolvedValue(failure("fatal: Authentication failed"));
    renderControls();

    await user.click(await button("Pull"));

    expect(await screen.findByRole("alert")).toHaveTextContent("fatal: Authentication failed");
    await user.click(await button("Run in terminal"));
    expect(mocked.runGitInTerminal).toHaveBeenCalledWith(7, "pull");
  });

  it("offers Run in terminal for Push too, whatever the error says", async () => {
    const user = userEvent.setup();
    mocked.runGitCommand.mockResolvedValue(failure("something unrecognisable"));
    renderControls();

    await user.click(await button("Push"));
    await user.click(await button("Run in terminal"));

    expect(mocked.runGitInTerminal).toHaveBeenCalledWith(7, "push");
  });

  it("keeps a failure that arrives while away, with its Run in terminal", async () => {
    const user = userEvent.setup();
    const finish = deferred<Result<null, GitCommandError>>();
    mocked.runGitCommand.mockReturnValue(finish.promise);
    const onFoundMissing = vi.fn();
    const { rerender } = render(<Controls workspace={ws} onFoundMissing={onFoundMissing} />);
    await user.click(await button("Pull"));

    rerender(<Controls workspace={ws} onFoundMissing={onFoundMissing} shown={false} />);
    await act(async () => finish.resolve(failure("fatal: no")));
    rerender(<Controls workspace={ws} onFoundMissing={onFoundMissing} />);

    expect(await screen.findByRole("alert")).toHaveTextContent("fatal: no");
    await user.click(await button("Run in terminal"));
    expect(mocked.runGitInTerminal).toHaveBeenCalledWith(7, "pull");
  });

  it("names the Workspace when the failure is from another one", async () => {
    const user = userEvent.setup();
    const finish = deferred<Result<null, GitCommandError>>();
    mocked.runGitCommand.mockReturnValue(finish.promise);
    const onFoundMissing = vi.fn();
    const { rerender } = render(<Controls workspace={ws} onFoundMissing={onFoundMissing} />);
    await user.click(await button("Pull"));

    rerender(<Controls workspace={{ ...ws, id: 8 }} onFoundMissing={onFoundMissing} />);
    await act(async () => finish.resolve(failure("fatal: no")));

    expect(await screen.findByRole("alert")).toHaveTextContent("app: fatal: no");
  });

  it("offers no terminal when git never ran", async () => {
    const user = userEvent.setup();
    mocked.runGitCommand.mockResolvedValue(refusal({ kind: "alreadyRunning", message: "Another git command is running" }));
    renderControls();

    await user.click(await button("Pull"));

    expect(await screen.findByRole("alert")).toHaveTextContent("Another git command is running");
    expect(screen.queryByRole("button", { name: "Run in terminal" })).not.toBeInTheDocument();
  });

  it("shows why the terminal could not open", async () => {
    const user = userEvent.setup();
    mocked.runGitCommand.mockResolvedValue(failure("fatal: no"));
    mocked.runGitInTerminal.mockResolvedValue({ ok: false, error: { kind: "terminal", message: "Could not start wt.exe" } });
    renderControls();

    await user.click(await button("Pull"));
    await user.click(await button("Run in terminal"));

    expect(await screen.findByRole("alert")).toHaveTextContent("Could not start wt.exe");
  });

  it("reports a folder that has gone as Missing", async () => {
    const user = userEvent.setup();
    mocked.runGitCommand.mockResolvedValue({
      ok: false,
      error: { kind: "workspaceMissing", message: "gone", workspaceId: 7, path: ws.path },
    });
    const { onFoundMissing } = renderControls();

    await user.click(await button("Pull"));

    await waitFor(() => expect(onFoundMissing).toHaveBeenCalled());
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });
});
