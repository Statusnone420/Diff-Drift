import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { RightPanel } from "../../src/components/RightPanel";

describe("RightPanel scope disclaimer", () => {
  it("states the real analysis scope, JSX and package.json included", () => {
    render(
      <RightPanel
        flags={[]}
        activeFlagId={null}
        onSelectFlag={vi.fn()}
        onDismissFlag={vi.fn()}
        onExport={vi.fn(async () => true)}
      />,
    );
    expect(screen.getByTestId("scope-note")).toHaveTextContent(
      "Heuristic checks on TS/TSX/JS/JSX and package.json drift — review, don't trust blindly.",
    );
    expect(screen.getByText("No active risk flags")).toBeInTheDocument();
  });
});
