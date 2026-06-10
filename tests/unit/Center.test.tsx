import { createRef } from "react";
import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { Center } from "../../src/components/Center";
import type { FileEntry } from "../../src/types";

function renderCenter(changedFiles: number, baselinePhrase: string, file: FileEntry | null = null) {
  render(
    <Center
      file={file}
      changedFiles={changedFiles}
      baselinePhrase={baselinePhrase}
      flagsById={{}}
      activeNodeId={null}
      pulseId={null}
      onToggleFlag={vi.fn()}
      onToggleReviewed={vi.fn()}
      registerRef={vi.fn()}
      scrollRef={createRef<HTMLDivElement>()}
    />,
  );
}

describe("Center empty states", () => {
  it("interpolates the baseline phrase instead of assuming HEAD", () => {
    renderCenter(0, "your last review (trust point ab12cd3)");
    expect(screen.getByText("No drift detected")).toBeInTheDocument();
    expect(screen.getByTestId("center-clean-sub")).toHaveTextContent(
      "Nothing has changed since your last review (trust point ab12cd3).",
    );
  });

  it("names the full analysis scope when changes exist but none are analyzable", () => {
    renderCenter(2, "the last commit (HEAD)");
    expect(screen.getByText("No analyzable drift detected")).toBeInTheDocument();
    expect(screen.getByTestId("center-clean-sub")).toHaveTextContent(
      "2 changed files found, but none are TypeScript, TSX, JavaScript, JSX, or package.json files Diff Drift can inspect.",
    );
  });

  it("renders a skipped panel for oversized files instead of claiming formatting-only", () => {
    const skipped: FileEntry = {
      id: "dist_bundle_js",
      name: "bundle.js",
      dir: "dist/",
      lang: "JavaScript",
      risks: 0,
      summary: "Skipped — file too large to analyze (3.0 MB > 2 MB)",
      skipped: true,
      changedNodes: 0,
      reviewedNodes: 0,
      nodes: [],
    };
    renderCenter(1, "the last commit (HEAD)", skipped);
    expect(screen.getByTestId("center-skipped")).toHaveTextContent(
      "This file changed but was not analyzed",
    );
    expect(screen.getByTestId("center-skipped")).toHaveTextContent("too large to analyze");
    expect(
      screen.queryByText(/Only formatting or whitespace changed/),
    ).not.toBeInTheDocument();
  });
});
