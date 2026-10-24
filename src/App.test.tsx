import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, vi } from "vitest";

import App from "./App";
import { api, type Workspace } from "@/lib/api";

vi.mock("@/lib/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/api")>();
  return {
    ...actual,
    api: {
      listWorkspaces: vi.fn(),
      addWorkspace: vi.fn(),
      pickFolder: vi.fn(),
      updateWorkspace: vi.fn(),
      setPinned: vi.fn(),
      removeWorkspace: vi.fn(),
    },
  };
});

const mocked = vi.mocked(api);

function workspace(overrides: Partial<Workspace> = {}): Workspace {
  return {
    id: 1,
    name: "code-orchestrate",
    path: "E:\\Programming\\code-orchestrate",
    pinned: false,
    note: null,
    createdAt: 1_700_000_000_000,
    tags: [],
    ...overrides,
  };
}

function allList() {
  return screen.getByRole("list", { name: "All" });
}

async function clickAddWorkspace(user: ReturnType<typeof userEvent.setup>) {
  const sidebar = screen.getByRole("complementary");
  await user.click(within(sidebar).getByRole("button", { name: "Add Workspace" }));
}

beforeEach(() => {
  vi.resetAllMocks();
  mocked.listWorkspaces.mockResolvedValue([]);
});

describe("App shell", () => {
  it("opens to an empty state that points to Add Workspace and Suggestions", async () => {
    render(<App />);

    expect(screen.getByRole("heading", { name: "Code Orchestrate" })).toBeInTheDocument();
    expect(await screen.findByRole("heading", { name: /no workspaces yet/i })).toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: /add workspace/i }).length).toBeGreaterThan(0);
    expect(screen.getAllByRole("button", { name: /suggestions/i }).length).toBeGreaterThan(0);
  });
});

describe("Workspace list", () => {
  it("lists saved Workspaces in the sidebar's All list", async () => {
    mocked.listWorkspaces.mockResolvedValue([
      workspace({ id: 1, name: "alpha", path: "E:\\alpha" }),
      workspace({ id: 2, name: "beta", path: "E:\\beta" }),
    ]);
    render(<App />);

    await waitFor(() => expect(within(allList()).getAllByRole("listitem")).toHaveLength(2));
    expect(within(allList()).getByText("alpha")).toBeInTheDocument();
    expect(within(allList()).getByText("beta")).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: /no workspaces yet/i })).not.toBeInTheDocument();
  });

  it("shows a Workspace's name and path when it is selected", async () => {
    mocked.listWorkspaces.mockResolvedValue([workspace()]);
    const user = userEvent.setup();
    render(<App />);

    await user.click(await screen.findByRole("button", { name: /code-orchestrate/ }));

    const main = screen.getByRole("main");
    expect(within(main).getByRole("heading", { name: "code-orchestrate" })).toBeInTheDocument();
    expect(within(main).getByText("E:\\Programming\\code-orchestrate")).toBeInTheDocument();
  });
});

describe("Add Workspace", () => {
  it("picks a folder, pre-fills the name from its last segment, and lists the new Workspace", async () => {
    mocked.pickFolder.mockResolvedValue("E:\\Programming\\my-project\\");
    const added = workspace({ id: 9, name: "My Project", path: "E:\\Programming\\my-project" });
    mocked.addWorkspace.mockResolvedValue({ ok: true, value: added });
    const user = userEvent.setup();
    render(<App />);

    await clickAddWorkspace(user);

    const dialog = await screen.findByRole("dialog", { name: "Add Workspace" });
    const name = within(dialog).getByLabelText("Name");
    expect(name).toHaveValue("my-project");
    expect(within(dialog).getByText("E:\\Programming\\my-project\\")).toBeInTheDocument();

    await user.clear(name);
    await user.type(name, "My Project");
    mocked.listWorkspaces.mockResolvedValue([added]);
    await user.click(within(dialog).getByRole("button", { name: "Save" }));

    expect(mocked.addWorkspace).toHaveBeenCalledWith("My Project", "E:\\Programming\\my-project\\");
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(within(allList()).getByText("My Project")).toBeInTheDocument();
  });

  it("does nothing when the folder picker is cancelled", async () => {
    mocked.pickFolder.mockResolvedValue(null);
    const user = userEvent.setup();
    render(<App />);

    await clickAddWorkspace(user);

    await waitFor(() => expect(mocked.pickFolder).toHaveBeenCalled());
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("rejects a duplicate inline, linking to the existing Workspace", async () => {
    const existing = workspace({ id: 3, name: "Orchestrate", path: "E:\\code" });
    mocked.listWorkspaces.mockResolvedValue([existing]);
    mocked.pickFolder.mockResolvedValue("e:\\CODE\\");
    mocked.addWorkspace.mockResolvedValue({
      ok: false,
      error: {
        kind: "pathTaken",
        message: "Already a Workspace: Orchestrate",
        workspaceId: 3,
        workspaceName: "Orchestrate",
      },
    });
    const user = userEvent.setup();
    render(<App />);

    await clickAddWorkspace(user);
    const dialog = await screen.findByRole("dialog", { name: "Add Workspace" });
    await user.click(within(dialog).getByRole("button", { name: "Save" }));

    const alert = await within(dialog).findByRole("alert");
    expect(alert).toHaveTextContent("Already a Workspace: Orchestrate");

    await user.click(within(alert).getByRole("button", { name: "Orchestrate" }));

    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(within(screen.getByRole("main")).getByRole("heading", { name: "Orchestrate" })).toBeInTheDocument();
  });

  it("shows why a path was rejected", async () => {
    mocked.pickFolder.mockResolvedValue("E:\\gone");
    mocked.addWorkspace.mockResolvedValue({
      ok: false,
      error: { kind: "notFound", message: "Folder not found: E:\\gone", path: "E:\\gone" },
    });
    const user = userEvent.setup();
    render(<App />);

    await clickAddWorkspace(user);
    const dialog = await screen.findByRole("dialog", { name: "Add Workspace" });
    await user.click(within(dialog).getByRole("button", { name: "Save" }));

    expect(await within(dialog).findByRole("alert")).toHaveTextContent("Folder not found: E:\\gone");
  });

  it("needs a name before saving", async () => {
    mocked.pickFolder.mockResolvedValue("E:\\x");
    const user = userEvent.setup();
    render(<App />);

    await clickAddWorkspace(user);
    const dialog = await screen.findByRole("dialog", { name: "Add Workspace" });
    await user.clear(within(dialog).getByLabelText("Name"));

    expect(within(dialog).getByRole("button", { name: "Save" })).toBeDisabled();
  });
});

describe("Loading Workspaces", () => {
  it("shows a load failure with a retry, not an empty state", async () => {
    mocked.listWorkspaces.mockRejectedValueOnce(new Error("disk I/O error"));
    mocked.listWorkspaces.mockResolvedValueOnce([workspace({ name: "alpha" })]);
    const user = userEvent.setup();
    render(<App />);

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("disk I/O error");
    expect(screen.queryByRole("heading", { name: /no workspaces yet/i })).not.toBeInTheDocument();

    await user.click(within(alert).getByRole("button", { name: "Retry" }));

    expect(await within(allList()).findByText("alpha")).toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });
});

async function selectWorkspace(user: ReturnType<typeof userEvent.setup>, name: string) {
  const sidebar = screen.getByRole("complementary");
  await user.click(await within(sidebar).findByRole("button", { name: new RegExp(`^${name}`) }));
}

async function openEdit(user: ReturnType<typeof userEvent.setup>) {
  await user.click(within(screen.getByRole("main")).getByRole("button", { name: "Edit" }));
  return screen.findByRole("dialog", { name: "Edit Workspace" });
}

describe("Workspace header", () => {
  it("shows the selected Workspace's name, path, tags, note and Edit", async () => {
    mocked.listWorkspaces.mockResolvedValue([
      workspace({ name: "Orchestrate", path: "E:\\code", tags: ["tauri", "personal"], note: "This app." }),
    ]);
    const user = userEvent.setup();
    render(<App />);

    await selectWorkspace(user, "Orchestrate");

    const main = screen.getByRole("main");
    expect(within(main).getByRole("heading", { name: "Orchestrate" })).toBeInTheDocument();
    expect(within(main).getByText("E:\\code")).toBeInTheDocument();
    const tags = within(main).getByRole("list", { name: "Tags" });
    expect(within(tags).getAllByRole("listitem").map((li) => li.textContent)).toEqual(["tauri", "personal"]);
    expect(within(main).getByText("This app.")).toBeInTheDocument();
    expect(within(main).getByRole("button", { name: "Edit" })).toBeInTheDocument();
  });
});

describe("Search and tag chips", () => {
  const listed = () =>
    within(screen.getByRole("complementary"))
      .queryAllByRole("listitem")
      .map((li) => li.textContent?.replace("Pinned", "").trim());

  beforeEach(() => {
    mocked.listWorkspaces.mockResolvedValue([
      workspace({ id: 1, name: "Orchestrate", path: "E:\\Programming\\orch", tags: ["Tauri", "personal"] }),
      workspace({ id: 2, name: "Wiki", path: "D:\\Writing\\wiki", tags: ["writing", "personal"], note: "Tuesday GAME" }),
      workspace({ id: 3, name: "Client site", path: "E:\\Work\\acme", tags: ["work"], pinned: true }),
    ]);
  });

  async function ready() {
    render(<App />);
    await waitFor(() => expect(listed()).toHaveLength(3));
  }

  it("matches name, path and note, ignoring case", async () => {
    const user = userEvent.setup();
    await ready();
    const search = screen.getByRole("searchbox", { name: "Search Workspaces" });

    await user.type(search, "orCHEST");
    expect(listed()).toEqual(["Orchestrate"]);

    await user.clear(search);
    await user.type(search, "e:\\work");
    expect(listed()).toEqual(["Client site"]);

    await user.clear(search);
    await user.type(search, "tuesday game");
    expect(listed()).toEqual(["Wiki"]);
  });

  it("shows one chip per tag in use, toggling and AND-combining", async () => {
    const user = userEvent.setup();
    await ready();
    const chips = screen.getByRole("group", { name: "Filter by tag" });
    expect(within(chips).getAllByRole("button").map((b) => b.textContent)).toEqual([
      "personal",
      "Tauri",
      "work",
      "writing",
    ]);

    const personal = within(chips).getByRole("button", { name: "personal" });
    await user.click(personal);
    expect(personal).toHaveAttribute("aria-pressed", "true");
    expect(listed()).toEqual(["Orchestrate", "Wiki"]);

    await user.click(within(chips).getByRole("button", { name: "writing" }));
    expect(listed()).toEqual(["Wiki"]);

    await user.click(personal);
    await user.click(within(chips).getByRole("button", { name: "writing" }));
    expect(listed()).toHaveLength(3);
  });

  it("narrows by search and chips together, matching tags case-insensitively", async () => {
    mocked.listWorkspaces.mockResolvedValue([
      workspace({ id: 1, name: "One", path: "E:\\one", tags: ["Rust"] }),
      workspace({ id: 2, name: "Two", path: "E:\\two", tags: ["rust"] }),
      workspace({ id: 3, name: "Three", path: "E:\\three" }),
    ]);
    const user = userEvent.setup();
    render(<App />);
    await waitFor(() => expect(listed()).toHaveLength(3));

    await user.click(within(screen.getByRole("group", { name: "Filter by tag" })).getByRole("button", { name: "Rust" }));
    expect(listed()).toEqual(["One", "Two"]);

    await user.type(screen.getByRole("searchbox", { name: "Search Workspaces" }), "two");
    expect(listed()).toEqual(["Two"]);
  });
});

describe("Pinning", () => {
  function pinnedList() {
    return screen.getByRole("list", { name: "Pinned" });
  }

  it("lists a pinned Workspace under Pinned and not under All", async () => {
    mocked.listWorkspaces.mockResolvedValue([
      workspace({ id: 1, name: "Starred", path: "E:\\s", pinned: true }),
      workspace({ id: 2, name: "Plain", path: "E:\\p" }),
    ]);
    render(<App />);

    const pinned = await screen.findByRole("list", { name: "Pinned" });
    expect(within(pinned).getByText("Starred")).toBeInTheDocument();
    expect(within(allList()).queryByText("Starred")).not.toBeInTheDocument();
    expect(within(allList()).getByText("Plain")).toBeInTheDocument();
  });

  it("has no Pinned list when nothing is pinned", async () => {
    mocked.listWorkspaces.mockResolvedValue([workspace({ name: "Plain" })]);
    render(<App />);

    await within(await screen.findByRole("list", { name: "All" })).findByText("Plain");
    expect(screen.queryByRole("list", { name: "Pinned" })).not.toBeInTheDocument();
  });

  it("pins and unpins from the header", async () => {
    const ws = workspace({ id: 3, name: "Mine" });
    mocked.listWorkspaces.mockResolvedValue([ws]);
    mocked.setPinned.mockResolvedValue({ ok: true, value: { ...ws, pinned: true } });
    const user = userEvent.setup();
    render(<App />);

    await selectWorkspace(user, "Mine");
    mocked.listWorkspaces.mockResolvedValue([{ ...ws, pinned: true }]);
    await user.click(within(screen.getByRole("main")).getByRole("button", { name: "Pin" }));

    expect(mocked.setPinned).toHaveBeenCalledWith(3, true);
    expect(await within(pinnedList()).findByText("Mine")).toBeInTheDocument();

    mocked.setPinned.mockResolvedValue({ ok: true, value: ws });
    mocked.listWorkspaces.mockResolvedValue([ws]);
    await user.click(within(screen.getByRole("main")).getByRole("button", { name: "Unpin" }));

    expect(mocked.setPinned).toHaveBeenLastCalledWith(3, false);
    expect(await within(allList()).findByText("Mine")).toBeInTheDocument();
  });
});

describe("Edit Workspace", () => {
  it("changes name, note and tags and saves them", async () => {
    const ws = workspace({ id: 4, name: "Old", tags: ["keep", "drop"], note: "old note" });
    mocked.listWorkspaces.mockResolvedValue([ws]);
    const edited = { ...ws, name: "New", note: "new note", tags: ["keep", "added"] };
    mocked.updateWorkspace.mockResolvedValue({ ok: true, value: edited });
    const user = userEvent.setup();
    render(<App />);

    await selectWorkspace(user, "Old");
    const dialog = await openEdit(user);

    const name = within(dialog).getByLabelText("Name");
    expect(name).toHaveValue("Old");
    await user.clear(name);
    await user.type(name, "New");
    const note = within(dialog).getByLabelText("Note");
    await user.clear(note);
    await user.type(note, "new note");
    await user.click(within(dialog).getByRole("button", { name: "Remove tag drop" }));
    await user.type(within(dialog).getByLabelText("Add tag"), "added{Enter}");

    mocked.listWorkspaces.mockResolvedValue([edited]);
    await user.click(within(dialog).getByRole("button", { name: "Save" }));

    expect(mocked.updateWorkspace).toHaveBeenCalledWith(4, {
      name: "New",
      note: "new note",
      tags: ["keep", "added"],
    });
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(within(screen.getByRole("main")).getByRole("heading", { name: "New" })).toBeInTheDocument();
  });

  it("saves a tag still being typed", async () => {
    const ws = workspace({ id: 4, name: "Old" });
    mocked.listWorkspaces.mockResolvedValue([ws]);
    mocked.updateWorkspace.mockResolvedValue({ ok: true, value: ws });
    const user = userEvent.setup();
    render(<App />);

    await selectWorkspace(user, "Old");
    const dialog = await openEdit(user);
    await user.type(within(dialog).getByLabelText("Add tag"), "typed");
    await user.click(within(dialog).getByRole("button", { name: "Save" }));

    expect(mocked.updateWorkspace).toHaveBeenCalledWith(4, { name: "Old", note: "", tags: ["typed"] });
  });

  it("needs a name before saving", async () => {
    mocked.listWorkspaces.mockResolvedValue([workspace({ name: "Old" })]);
    const user = userEvent.setup();
    render(<App />);

    await selectWorkspace(user, "Old");
    const dialog = await openEdit(user);
    await user.clear(within(dialog).getByLabelText("Name"));

    expect(within(dialog).getByRole("button", { name: "Save" })).toBeDisabled();
  });

  it("ignores a tag that is already on the Workspace in another case", async () => {
    mocked.listWorkspaces.mockResolvedValue([workspace({ name: "Old", tags: ["Rust"] })]);
    const user = userEvent.setup();
    render(<App />);

    await selectWorkspace(user, "Old");
    const dialog = await openEdit(user);
    await user.type(within(dialog).getByLabelText("Add tag"), "rust{Enter}");

    expect(within(dialog).getAllByRole("button", { name: /^Remove tag/ })).toHaveLength(1);
  });

  it("offers tags already in use as autocomplete, once each", async () => {
    mocked.listWorkspaces.mockResolvedValue([
      workspace({ id: 1, name: "A", path: "E:\\a", tags: ["Rust", "work"] }),
      workspace({ id: 2, name: "B", path: "E:\\b", tags: ["rust"] }),
    ]);
    const user = userEvent.setup();
    render(<App />);

    await selectWorkspace(user, "B");
    const dialog = await openEdit(user);

    const input = within(dialog).getByLabelText("Add tag") as HTMLInputElement;
    expect([...input.list!.options].map((o) => o.value)).toEqual(["work"]);
  });

  it("removes the Workspace only after confirming", async () => {
    mocked.listWorkspaces.mockResolvedValue([
      workspace({ id: 5, name: "Doomed", path: "E:\\doomed" }),
      workspace({ id: 6, name: "Kept", path: "E:\\kept" }),
    ]);
    mocked.removeWorkspace.mockResolvedValue({ ok: true, value: null });
    const user = userEvent.setup();
    render(<App />);

    await selectWorkspace(user, "Doomed");
    const dialog = await openEdit(user);
    await user.click(within(dialog).getByRole("button", { name: "Remove Workspace" }));
    expect(mocked.removeWorkspace).not.toHaveBeenCalled();

    mocked.listWorkspaces.mockResolvedValue([workspace({ id: 6, name: "Kept", path: "E:\\kept" })]);
    await user.click(within(dialog).getByRole("button", { name: "Remove" }));

    expect(mocked.removeWorkspace).toHaveBeenCalledWith(5);
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    await waitFor(() => expect(within(allList()).queryByText("Doomed")).not.toBeInTheDocument());
    expect(within(screen.getByRole("main")).queryByRole("heading", { name: "Doomed" })).not.toBeInTheDocument();
  });

  it("can back out of removing", async () => {
    mocked.listWorkspaces.mockResolvedValue([workspace({ name: "Safe" })]);
    const user = userEvent.setup();
    render(<App />);

    await selectWorkspace(user, "Safe");
    const dialog = await openEdit(user);
    await user.click(within(dialog).getByRole("button", { name: "Remove Workspace" }));
    await user.click(within(dialog).getByRole("button", { name: "Keep" }));

    expect(mocked.removeWorkspace).not.toHaveBeenCalled();
    expect(within(dialog).getByRole("button", { name: "Save" })).toBeInTheDocument();
  });

  it("shows a save failure", async () => {
    mocked.listWorkspaces.mockResolvedValue([workspace({ name: "Old" })]);
    mocked.updateWorkspace.mockResolvedValue({
      ok: false,
      error: { kind: "internal", message: "disk full" },
    });
    const user = userEvent.setup();
    render(<App />);

    await selectWorkspace(user, "Old");
    const dialog = await openEdit(user);
    await user.click(within(dialog).getByRole("button", { name: "Save" }));

    expect(await within(dialog).findByRole("alert")).toHaveTextContent("disk full");
  });
});
