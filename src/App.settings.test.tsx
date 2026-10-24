import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, vi } from "vitest";

import App from "./App";
import { api, type SettingsSnapshot, type Workspace } from "@/lib/api";

vi.mock("@/lib/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/api")>();
  return {
    ...actual,
    api: {
      listWorkspaces: vi.fn(),
      addWorkspace: vi.fn(),
      pickFolder: vi.fn(),
      getSettings: vi.fn(),
      setSetting: vi.fn(),
      resetSettings: vi.fn(),
      agentStatus: vi.fn(),
      launchSession: vi.fn(),
    },
  };
});

const mocked = vi.mocked(api);

const defaults: SettingsSnapshot["values"] = {
  theme: "system",
  "terminal.preset": "windowsTerminal",
  "terminal.windowsTerminal.target": "newWindow",
  "terminal.custom": null,
};

function snapshot(overrides: Partial<SettingsSnapshot> = {}): SettingsSnapshot {
  return { values: defaults, warnings: {}, fileError: null, choices: {}, ...overrides };
}

const ws: Workspace = {
  id: 1,
  name: "alpha",
  path: "E:\\alpha",
  pinned: false,
  note: null,
  createdAt: 1_700_000_000_000,
};

async function openSettings(user: ReturnType<typeof userEvent.setup>) {
  const sidebar = screen.getByRole("complementary");
  await user.click(within(sidebar).getByRole("button", { name: "Settings" }));
  return screen.findByRole("heading", { name: "Settings", level: 2 });
}

function themeGroup() {
  return screen.getByRole("radiogroup", { name: "Theme" });
}

beforeEach(() => {
  vi.resetAllMocks();
  mocked.listWorkspaces.mockResolvedValue([ws]);
  mocked.getSettings.mockResolvedValue({ ok: true, value: snapshot() });
  mocked.agentStatus.mockResolvedValue({
    id: "claude-code",
    displayName: "Claude Code",
    capabilities: { continue: true, resume: true, sessionHistory: true },
    executable: "claude.exe",
  });
});

describe("Settings page", () => {
  it("opens from the gear in the detail pane, with the Appearance section", async () => {
    const user = userEvent.setup();
    render(<App />);

    await openSettings(user);

    const main = screen.getByRole("main");
    expect(within(main).getByRole("heading", { name: "Appearance" })).toBeInTheDocument();
    expect(within(themeGroup()).getByRole("radio", { name: "System" })).toBeChecked();
  });

  it("autosaves a theme change and shows the saved value", async () => {
    mocked.setSetting.mockResolvedValue({ ok: true, value: snapshot({ values: { ...defaults, theme: "dark" } }) });
    const user = userEvent.setup();
    render(<App />);
    await openSettings(user);

    await user.click(within(themeGroup()).getByRole("radio", { name: "Dark" }));

    expect(mocked.setSetting).toHaveBeenCalledWith("theme", "dark");
    await waitFor(() => expect(within(themeGroup()).getByRole("radio", { name: "Dark" })).toBeChecked());
  });

  it("shows why a save failed", async () => {
    mocked.setSetting.mockResolvedValue({
      ok: false,
      error: { kind: "internal", message: "disk full" },
    });
    const user = userEvent.setup();
    render(<App />);
    await openSettings(user);

    await user.click(within(themeGroup()).getByRole("radio", { name: "Light" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("disk full");
  });

  it("is left by pressing Esc", async () => {
    const user = userEvent.setup();
    render(<App />);
    await openSettings(user);

    await user.keyboard("{Escape}");

    expect(screen.queryByRole("heading", { name: "Settings" })).not.toBeInTheDocument();
  });

  it("is left by selecting a Workspace", async () => {
    const user = userEvent.setup();
    render(<App />);
    await openSettings(user);

    await user.click(within(screen.getByRole("list", { name: "All" })).getByRole("button", { name: "alpha" }));

    expect(screen.queryByRole("heading", { name: "Settings" })).not.toBeInTheDocument();
    expect(within(screen.getByRole("main")).getByRole("heading", { name: "alpha" })).toBeInTheDocument();
  });

  it("warns inline when a value in the file was ignored", async () => {
    mocked.getSettings.mockResolvedValue({
      ok: true,
      value: snapshot({ warnings: { theme: 'Using the default: "purple" is not one of: system, light, dark' } }),
    });
    const user = userEvent.setup();
    render(<App />);
    await openSettings(user);

    expect(screen.getByText(/"purple" is not one of/)).toBeInTheDocument();
    expect(within(themeGroup()).getByRole("radio", { name: "System" })).toBeChecked();
  });

  it("shows a banner for an invalid file, blocks edits, and can reset to defaults", async () => {
    mocked.getSettings.mockResolvedValue({
      ok: true,
      value: snapshot({ fileError: "trailing comma at line 1 column 19" }),
    });
    mocked.resetSettings.mockResolvedValue({ ok: true, value: snapshot() });
    const user = userEvent.setup();
    render(<App />);
    await openSettings(user);

    const banner = screen.getByRole("alert");
    expect(banner).toHaveTextContent("trailing comma at line 1 column 19");
    expect(within(themeGroup()).getByRole("radio", { name: "Dark" })).toBeDisabled();

    await user.click(within(banner).getByRole("button", { name: "Reset to defaults" }));

    expect(mocked.resetSettings).toHaveBeenCalled();
    await waitFor(() => expect(screen.queryByRole("alert")).not.toBeInTheDocument());
    expect(within(themeGroup()).getByRole("radio", { name: "Dark" })).toBeEnabled();
  });
});

describe("settings.json reloading", () => {
  it("is read on startup", async () => {
    render(<App />);
    await waitFor(() => expect(mocked.getSettings).toHaveBeenCalledTimes(1));
  });

  it("is re-read when the window gains focus", async () => {
    const user = userEvent.setup();
    render(<App />);
    await openSettings(user);
    mocked.getSettings.mockResolvedValue({ ok: true, value: snapshot({ values: { ...defaults, theme: "light" } }) });

    await act(async () => {
      fireEvent.focus(window);
    });

    await waitFor(() => expect(within(themeGroup()).getByRole("radio", { name: "Light" })).toBeChecked());
  });

  it("ignores a re-read that started before a newer save finished", async () => {
    const user = userEvent.setup();
    render(<App />);
    await openSettings(user);

    let finishStaleRead!: () => void;
    mocked.getSettings.mockReturnValue(
      new Promise((resolve) => {
        finishStaleRead = () => resolve({ ok: true, value: snapshot() });
      }),
    );
    await act(async () => {
      fireEvent.focus(window);
    });
    mocked.setSetting.mockResolvedValue({ ok: true, value: snapshot({ values: { ...defaults, theme: "dark" } }) });
    await user.click(within(themeGroup()).getByRole("radio", { name: "Dark" }));
    await waitFor(() => expect(within(themeGroup()).getByRole("radio", { name: "Dark" })).toBeChecked());

    await act(async () => finishStaleRead());

    expect(within(themeGroup()).getByRole("radio", { name: "Dark" })).toBeChecked();
  });

  it("is opened from a failed launch's Open Settings action", async () => {
    mocked.launchSession.mockResolvedValue({
      ok: false,
      error: { kind: "executableNotFound", message: "claude was not found", name: "claude" },
    });
    const user = userEvent.setup();
    render(<App />);
    await user.click(await screen.findByRole("button", { name: "alpha" }));

    const button = await screen.findByRole("button", { name: "New Session" });
    await waitFor(() => expect(button).toBeEnabled());
    await user.click(button);
    await user.click(await screen.findByRole("button", { name: "Open Settings" }));

    expect(await screen.findByRole("heading", { name: "Settings", level: 2 })).toBeInTheDocument();
  });
});
