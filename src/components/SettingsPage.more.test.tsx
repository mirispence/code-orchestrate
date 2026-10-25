import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, vi } from "vitest";

import { SettingsPage } from "./SettingsPage";
import { api, type ClaudeCodeStatus, type SettingsSnapshot, type SettingValues } from "@/lib/api";
import type { SettingsState } from "@/lib/useSettings";

vi.mock("@/lib/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/api")>();
  return {
    ...actual,
    api: {
      testTerminal: vi.fn(),
      claudeCodeStatus: vi.fn(),
      listDismissedPaths: vi.fn(),
      restoreDismissedPath: vi.fn(),
      rescanTranscripts: vi.fn(),
      openDataFolder: vi.fn(),
    },
  };
});

const mocked = vi.mocked(api);

function snapshot(values: Partial<SettingValues> = {}): SettingsSnapshot {
  return {
    values: {
      theme: "system",
      "terminal.preset": "windowsTerminal",
      "terminal.windowsTerminal.target": "newWindow",
      "terminal.custom": null,
      "agents.claude-code.executable": "",
      "agents.claude-code.transcriptRoot": "",
      ...values,
    },
    warnings: {},
    fileError: null,
    choices: {},
  };
}

const found: ClaudeCodeStatus = {
  executable: { state: "found", path: "C:\\bin\\claude.exe", version: "2.1.0 (Claude Code)" },
  transcriptRoot: { state: "ok", path: "C:\\Users\\me\\.claude" },
};

function renderPage(snap: SettingsSnapshot = snapshot(), onRestored = vi.fn()) {
  const change = vi.fn().mockResolvedValue(undefined);
  const settings: SettingsState = { snapshot: snap, error: null, change, reset: vi.fn() };
  const view = render(<SettingsPage settings={settings} onClose={() => {}} onRestored={onRestored} />);
  const rerender = (next: SettingsSnapshot) =>
    view.rerender(
      <SettingsPage settings={{ ...settings, snapshot: next }} onClose={() => {}} onRestored={onRestored} />,
    );
  return { change, rerender, onRestored };
}

function section(name: string) {
  return screen.getByRole("region", { name });
}

beforeEach(() => {
  vi.resetAllMocks();
  mocked.claudeCodeStatus.mockResolvedValue(found);
  mocked.listDismissedPaths.mockResolvedValue({ ok: true, value: [] });
  mocked.restoreDismissedPath.mockResolvedValue({ ok: true, value: null });
  mocked.rescanTranscripts.mockResolvedValue(undefined);
  mocked.openDataFolder.mockResolvedValue({ ok: true, value: null });
});

describe("Settings › Agents › Claude Code", () => {
  it("probes the executable when Settings opens and shows the resolved path and version", async () => {
    renderPage();

    const cc = section("Agents › Claude Code");
    expect(await within(cc).findByText(/C:\\bin\\claude\.exe/)).toHaveTextContent("2.1.0 (Claude Code)");
    expect(within(cc).getByText(/C:\\Users\\me\\\.claude/)).toBeInTheDocument();
    expect(mocked.claudeCodeStatus).toHaveBeenCalledWith(true);
  });

  it("shows Not found and the missing projects folder", async () => {
    mocked.claudeCodeStatus.mockResolvedValue({
      executable: { state: "notFound" },
      transcriptRoot: { state: "noProjects", path: "D:\\cc" },
    });
    renderPage();

    const cc = section("Agents › Claude Code");
    expect(await within(cc).findByText("Not found")).toBeInTheDocument();
    expect(within(cc).getByText("Transcript root has no projects folder")).toBeInTheDocument();
  });

  it("saves an executable override when the field is left, and blank restores the default", async () => {
    const user = userEvent.setup();
    const { change } = renderPage();

    const box = screen.getByRole("textbox", { name: "Executable path" });
    expect(box).toHaveAttribute("placeholder", "claude on PATH");
    await user.type(box, "D:\\tools\\claude.exe");
    await user.tab();

    expect(change).toHaveBeenCalledWith("agents.claude-code.executable", "D:\\tools\\claude.exe");
  });

  it("saves a transcript root override on Enter", async () => {
    const user = userEvent.setup();
    const { change } = renderPage();

    await user.type(screen.getByRole("textbox", { name: "Transcript root" }), "D:\\cc{Enter}");

    expect(change).toHaveBeenCalledWith("agents.claude-code.transcriptRoot", "D:\\cc");
  });

  it("saves an override when Esc leaves the page from the field", async () => {
    const user = userEvent.setup();
    const { change } = renderPage();

    await user.type(screen.getByRole("textbox", { name: "Executable path" }), "D:\\c.exe{Escape}");

    expect(change).toHaveBeenCalledWith("agents.claude-code.executable", "D:\\c.exe");
  });

  it("re-checks the status when an override changes", async () => {
    const { rerender } = renderPage();
    await waitFor(() => expect(mocked.claudeCodeStatus).toHaveBeenCalledTimes(1));

    rerender(snapshot({ "agents.claude-code.executable": "D:\\other.exe" }));

    await waitFor(() => expect(mocked.claudeCodeStatus).toHaveBeenCalledTimes(2));
  });
});

describe("Settings › Suggestions", () => {
  it("lists dismissed paths and restores one", async () => {
    const user = userEvent.setup();
    mocked.listDismissedPaths
      .mockResolvedValueOnce({ ok: true, value: ["E:\\old", "E:\\scratch"] })
      .mockResolvedValue({ ok: true, value: ["E:\\scratch"] });
    const { onRestored } = renderPage();

    const list = within(section("Suggestions"));
    const row = (await list.findByText("E:\\old")).closest("li")!;
    await user.click(within(row as HTMLElement).getByRole("button", { name: "Restore" }));

    expect(mocked.restoreDismissedPath).toHaveBeenCalledWith("E:\\old");
    await waitFor(() => expect(list.queryByText("E:\\old")).not.toBeInTheDocument());
    expect(list.getByText("E:\\scratch")).toBeInTheDocument();
    expect(onRestored).toHaveBeenCalled();
  });

  it("says when nothing is dismissed", async () => {
    renderPage();
    expect(await within(section("Suggestions")).findByText("No dismissed paths.")).toBeInTheDocument();
  });
});

describe("Settings › Data", () => {
  it("rescans transcripts and opens the data folder", async () => {
    const user = userEvent.setup();
    renderPage();

    const data = within(section("Data"));
    await user.click(data.getByRole("button", { name: "Rescan transcripts" }));
    await user.click(data.getByRole("button", { name: "Open data folder" }));

    expect(mocked.rescanTranscripts).toHaveBeenCalled();
    expect(mocked.openDataFolder).toHaveBeenCalled();
  });
});
