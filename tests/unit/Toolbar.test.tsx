import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Toolbar } from "../../src/components/Toolbar";
import { makeSession } from "./helpers";

function renderToolbar(session = makeSession(), onSetBaseline = vi.fn()) {
  render(
    <Toolbar
      session={session}
      onSwitchRepo={vi.fn()}
      onDismissAll={vi.fn()}
      onToggleApprove={vi.fn()}
      onSetBaseline={onSetBaseline}
    />,
  );
  return { onSetBaseline };
}

describe("Toolbar baseline picker", () => {
  it("is a labeled control with plain-language options", () => {
    renderToolbar();
    const select = screen.getByLabelText("Review changes since");
    expect(select).toHaveValue("head");
    const labels = screen.getAllByRole("option").map((o) => o.textContent);
    expect(labels).toEqual([
      "Last commit (HEAD)",
      "Last review — none pinned yet",
      "Branch start (merge-base)",
      "Custom ref…",
    ]);
    expect(screen.getByRole("option", { name: "Last review — none pinned yet" })).toBeDisabled();
  });

  it("unlocks and names the trust point once one is pinned", () => {
    renderToolbar(makeSession({ trustPoint: "ab12cd3" }));
    const trust = screen.getByRole("option", { name: "Last review (trust point ab12cd3)" });
    expect(trust).toBeEnabled();
  });

  it("fires onSetBaseline when a custom ref is typed and confirmed", async () => {
    const user = userEvent.setup();
    const { onSetBaseline } = renderToolbar();
    await user.selectOptions(screen.getByLabelText("Review changes since"), "custom");
    const input = screen.getByLabelText("Custom baseline ref");
    await user.type(input, "v1.2.3{Enter}");
    expect(onSetBaseline).toHaveBeenCalledWith("v1.2.3");
  });
});

describe("Toolbar status copy", () => {
  it("keeps the clean pill honest about a non-HEAD baseline", () => {
    renderToolbar(
      makeSession({
        baselineSpec: "trust-point",
        baselineLabel: "trust point @ ab12cd3",
        trustPoint: "ab12cd3",
        changedFiles: 0,
        riskCount: 0,
      }),
    );
    expect(screen.getByTestId("summary-pill")).toHaveTextContent(
      "Clean — no changes since your last review (trust point ab12cd3)",
    );
  });

  it("reports flag counts when drift has risks", () => {
    renderToolbar(makeSession({ changedFiles: 3, riskCount: 2, fileCount: 2 }));
    expect(screen.getByTestId("summary-pill")).toHaveTextContent("2 flags in 2 files");
  });
});
