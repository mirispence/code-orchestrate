import { render, screen } from "@testing-library/react";
import App from "./App";

describe("App shell", () => {
  it("opens to an empty state that points to Add Workspace and Suggestions", () => {
    render(<App />);

    expect(screen.getByRole("heading", { name: "Code Orchestrate" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: /no workspaces yet/i })).toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: /add workspace/i }).length).toBeGreaterThan(0);
    expect(screen.getAllByRole("button", { name: /suggestions/i }).length).toBeGreaterThan(0);
  });
});
