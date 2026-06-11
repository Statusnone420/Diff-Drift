import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { RightPanel } from "../../src/components/RightPanel";

describe("RightPanel scope disclaimer", () => {
  it("states the real analysis scope, JSX and package.json included", () => {
    render(
      <RightPanel
        flags={[]}
        changedNodes={0}
        reviewedNodes={0}
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

  it("does not imply no-flag drift is fully reviewed", () => {
    render(
      <RightPanel
        flags={[]}
        changedNodes={8}
        reviewedNodes={0}
        activeFlagId={null}
        onSelectFlag={vi.fn()}
        onDismissFlag={vi.fn()}
        onExport={vi.fn(async () => true)}
      />,
    );
    expect(screen.getByText("No active risk flags")).toBeInTheDocument();
    expect(screen.getByText("8 changed nodes still need review.")).toBeInTheDocument();
  });
});
