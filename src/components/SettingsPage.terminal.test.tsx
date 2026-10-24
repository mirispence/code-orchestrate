import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, vi } from "vitest";

import { SettingsPage } from "./SettingsPage";
import { api, type SettingsSnapshot, type SettingValues } from "@/lib/api";
import type { SettingsState } from "@/lib/useSettings";

vi.mock("@/lib/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/api")>();
  return { ...actual, api: { testTerminal: vi.fn() } };
});

const mocked = vi.mocked(api);

const WINDOWS_PRESETS = ["windowsTerminal", "console", "custom"] as const;

function snapshot(values: Partial<SettingValues> = {}, presets: readonly string[] = WINDOWS_PRESETS): SettingsSnapshot {
  return {
    values: {
      theme: "system",
      "terminal.preset": "windowsTerminal",
      "terminal.windowsTerminal.target": "newWindow",
      "terminal.custom": null,
      ...values,
    },
    warnings: {},
    fileError: null,
    choices: {
      theme: ["system", "light", "dark"],
      "terminal.preset": [...presets],
      "terminal.windowsTerminal.target": ["newWindow", "newTab"],
    },
  };
}

function renderPage(snap: SettingsSnapshot) {
  const change = vi.fn().mockResolvedValue(undefined);
  const settings: SettingsState = { snapshot: snap, error: null, change, reset: vi.fn() };
  render(<SettingsPage settings={settings} onClose={() => {}} />);
  return { change };
}

function presetGroup() {
  return screen.getByRole("radiogroup", { name: "Preset" });
}

function templateBox() {
  return screen.getByRole("textbox", { name: "Custom terminal" });
}

beforeEach(() => {
  vi.resetAllMocks();
});

describe("Settings › Terminal", () => {
  it("offers Windows Terminal, Console window and Custom on Windows", () => {
    renderPage(snapshot());

    const radios = within(presetGroup()).getAllByRole("radio");
    expect(radios.map((r) => r.textContent)).toEqual(["Windows Terminal", "Console window", "Custom"]);
    expect(within(presetGroup()).getByRole("radio", { name: "Windows Terminal" })).toBeChecked();
  });

  it("offers only Custom where the platform has no other preset", () => {
    renderPage(snapshot({ "terminal.preset": "custom" }, ["custom"]));

    const radios = within(presetGroup()).getAllByRole("radio");
    expect(radios.map((r) => r.textContent)).toEqual(["Custom"]);
  });

  it("autosaves the preset", async () => {
    const user = userEvent.setup();
    const { change } = renderPage(snapshot());

    await user.click(within(presetGroup()).getByRole("radio", { name: "Console window" }));

    expect(change).toHaveBeenCalledWith("terminal.preset", "console");
  });

  it("lets Windows Terminal open a new tab in the most recent window", async () => {
    const user = userEvent.setup();
    const { change } = renderPage(snapshot());

    const target = screen.getByRole("radiogroup", { name: "Open in" });
    await user.click(within(target).getByRole("radio", { name: "New tab in most recent window" }));

    expect(change).toHaveBeenCalledWith("terminal.windowsTerminal.target", "newTab");
  });

  it("hides the Windows Terminal option for other presets", () => {
    renderPage(snapshot({ "terminal.preset": "console" }));

    expect(screen.queryByRole("radiogroup", { name: "Open in" })).not.toBeInTheDocument();
  });

  it("shows the saved custom template as one quoted line", () => {
    renderPage(
      snapshot({ "terminal.preset": "custom", "terminal.custom": ["C:\\Program Files\\t.exe", "-d", "{cwd}", "{command}"] }),
    );

    expect(templateBox()).toHaveValue('"C:\\Program Files\\t.exe" -d {cwd} {command}');
  });

  it("saves an edited template as an argv array when the field is left", async () => {
    const user = userEvent.setup();
    const { change } = renderPage(snapshot({ "terminal.preset": "custom" }));

    await user.type(templateBox(), 'alacritty --working-directory {{cwd} -e {{command}');
    await user.tab();

    expect(change).toHaveBeenCalledWith("terminal.custom", ["alacritty", "--working-directory", "{cwd}", "-e", "{command}"]);
  });

  it("saves an edited template when Esc leaves the page from the field", async () => {
    const user = userEvent.setup();
    const { change } = renderPage(snapshot({ "terminal.preset": "custom" }));

    await user.type(templateBox(), "wezterm start {{command}");
    await user.keyboard("{Escape}");

    expect(change).toHaveBeenCalledWith("terminal.custom", ["wezterm", "start", "{command}"]);
  });

  it("rejects a template without {command} and does not save it", async () => {
    const user = userEvent.setup();
    const { change } = renderPage(snapshot({ "terminal.preset": "custom" }));

    await user.type(templateBox(), "alacritty --working-directory {{cwd}");
    await user.tab();

    expect(screen.getByText(/must contain \{command\}/)).toBeInTheDocument();
    expect(change).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "Test" })).toBeDisabled();
  });

  it("clears the template when the line is emptied", async () => {
    const user = userEvent.setup();
    const { change } = renderPage(snapshot({ "terminal.preset": "custom", "terminal.custom": ["{command}"] }));

    await user.clear(templateBox());
    await user.tab();

    expect(change).toHaveBeenCalledWith("terminal.custom", null);
  });

  it("tests the template by launching claude --version through it", async () => {
    mocked.testTerminal.mockResolvedValue({ ok: true, value: undefined });
    const user = userEvent.setup();
    renderPage(snapshot({ "terminal.preset": "custom", "terminal.custom": ["wezterm", "start", "{command}"] }));

    await user.click(screen.getByRole("button", { name: "Test" }));

    expect(mocked.testTerminal).toHaveBeenCalledWith(["wezterm", "start", "{command}"]);
    expect(await screen.findByText(/claude --version/)).toBeInTheDocument();
  });

  it("shows why a test launch failed", async () => {
    mocked.testTerminal.mockResolvedValue({
      ok: false,
      error: { kind: "spawnFailed", message: "Could not start wezterm: not found", program: "wezterm", osError: "not found" },
    });
    const user = userEvent.setup();
    renderPage(snapshot({ "terminal.preset": "custom", "terminal.custom": ["wezterm", "{command}"] }));

    await user.click(screen.getByRole("button", { name: "Test" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("Could not start wezterm: not found");
  });
});
