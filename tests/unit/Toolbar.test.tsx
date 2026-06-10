import { describe, expect, it, vi } from "vitest";
import { render, screen, within } from "@testing-library/react";
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

describe("Toolbar analysis scope", () => {
  it("shows a compact scope trigger with explanatory choices", async () => {
    const user = userEvent.setup();
    renderToolbar();
    const trigger = screen.getByTestId("scope-trigger");
    expect(trigger).toHaveTextContent("ScopeCurrent workUncommitted changes");

    await user.click(trigger);
    const dialog = screen.getByRole("dialog", { name: "Analysis scope" });
    expect(dialog).toHaveTextContent("Choose what the current working tree is compared against.");
    expect(within(dialog).getByRole("button", { name: /Current work/ })).toHaveTextContent(
      "Uncommitted changes since the last commit.",
    );
    expect(within(dialog).getByRole("button", { name: /Since last review/ })).toBeDisabled();
    expect(within(dialog).getByRole("button", { name: /Entire branch/ })).toHaveTextContent(
      "Everything this branch adds over the default branch.",
    );
  });

  it("names the selected last-review scope once one is pinned", () => {
    renderToolbar(
      makeSession({
        baselineSpec: "trust-point",
        baselineLabel: "trust point @ ab12cd3",
        trustPoint: "ab12cd3",
      }),
    );
    expect(screen.getByTestId("scope-trigger")).toHaveTextContent("ScopeSince last reviewAgent commits stay visible");
  });

  it("fires onSetBaseline when a scope is selected", async () => {
    const user = userEvent.setup();
    const { onSetBaseline } = renderToolbar();
    await user.click(screen.getByTestId("scope-trigger"));
    await user.click(screen.getByRole("button", { name: /Entire branch/ }));
    expect(onSetBaseline).toHaveBeenCalledWith("merge-base");
  });

  it("unlocks the last-review option once a trust point is pinned", async () => {
    const user = userEvent.setup();
    const { onSetBaseline } = renderToolbar(makeSession({ trustPoint: "ab12cd3" }));
    await user.click(screen.getByTestId("scope-trigger"));
    const trust = screen.getByRole("button", { name: /Since last review/ });
    expect(trust).toBeEnabled();
    expect(trust).toHaveTextContent("Trust point ab12cd3");
    await user.click(trust);
    expect(onSetBaseline).toHaveBeenCalledWith("trust-point");
  });

  it("fires onSetBaseline when a custom ref is typed and confirmed", async () => {
    const user = userEvent.setup();
    const { onSetBaseline } = renderToolbar();
    await user.click(screen.getByTestId("scope-trigger"));
    await user.click(screen.getByRole("button", { name: /Custom ref/ }));
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

  it("does not let skipped files read as fully clean when there are no flags", () => {
    renderToolbar(makeSession({ changedFiles: 1, riskCount: 0, fileCount: 1, skippedFiles: 1 }));
    expect(screen.getByTestId("summary-pill")).toHaveTextContent("No flags — 1 skipped file not analyzed");
  });

  it("keeps approved no-flag copy honest when files were skipped", () => {
    renderToolbar(
      makeSession({
        approved: true,
        approvedAt: "12:00",
        changedFiles: 1,
        riskCount: 0,
        fileCount: 1,
        skippedFiles: 1,
      }),
    );
    expect(screen.getByTestId("summary-pill")).toHaveTextContent(
      "Reviewed at 12:00 — no open flags; 1 skipped file not analyzed",
    );
  });

  it("keeps approved open-flag copy honest when files were skipped", () => {
    renderToolbar(
      makeSession({
        approved: true,
        approvedAt: "12:00",
        changedFiles: 3,
        riskCount: 2,
        fileCount: 2,
        skippedFiles: 1,
      }),
    );
    expect(screen.getByTestId("summary-pill")).toHaveTextContent(
      "Reviewed at 12:00 — 2 flags still open; 1 skipped file not analyzed",
    );
  });
});
