// Inline SVG icons + lookup maps, ported verbatim from the handoff `app-win.jsx`.
import type { ReactNode } from "react";
import type { Severity } from "../types";

export const Ico: Record<string, ReactNode> = {
  file: (
    <svg width="13" height="13" viewBox="0 0 16 16" fill="none">
      <path d="M4 1.5h5L13 5v9.5H4z" stroke="currentColor" strokeWidth="1.2" fill="none" />
      <path d="M9 1.5V5h4" stroke="currentColor" strokeWidth="1.2" fill="none" />
    </svg>
  ),
  branch: (
    <svg width="11" height="11" viewBox="0 0 16 16" fill="none">
      <circle cx="4" cy="3.5" r="1.8" stroke="currentColor" strokeWidth="1.2" />
      <circle cx="4" cy="12.5" r="1.8" stroke="currentColor" strokeWidth="1.2" />
      <circle cx="12" cy="3.5" r="1.8" stroke="currentColor" strokeWidth="1.2" />
      <path
        d="M4 5.3v5.4M12 5.3c0 3-2 4-5 4.2"
        stroke="currentColor"
        strokeWidth="1.2"
        fill="none"
      />
    </svg>
  ),
  chevron: (
    <svg width="12" height="12" viewBox="0 0 16 16" fill="none">
      <path
        d="M6 4l4 4-4 4"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  ),
  flag: (
    <svg width="11" height="11" viewBox="0 0 16 16" fill="none">
      <path
        d="M4 14V2.5M4 3h7l-1.4 2.2L11 7.5H4"
        stroke="currentColor"
        strokeWidth="1.3"
        strokeLinejoin="round"
        fill="none"
      />
    </svg>
  ),
  warn: (
    <svg width="11" height="11" viewBox="0 0 16 16" fill="none">
      <path
        d="M8 2l6.2 11H1.8z"
        stroke="currentColor"
        strokeWidth="1.3"
        strokeLinejoin="round"
        fill="none"
      />
      <path d="M8 6.4v3.1" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />
      <circle cx="8" cy="11.4" r="0.8" fill="currentColor" />
    </svg>
  ),
  jump: (
    <svg width="12" height="12" viewBox="0 0 16 16" fill="none">
      <path
        d="M3 8h8M8 4l4 4-4 4"
        stroke="currentColor"
        strokeWidth="1.4"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  ),
  eye: (
    <svg width="12" height="12" viewBox="0 0 16 16" fill="none">
      <path
        d="M1 8s2.6-4.4 7-4.4S15 8 15 8s-2.6 4.4-7 4.4S1 8 1 8z"
        stroke="currentColor"
        strokeWidth="1.2"
        fill="none"
        strokeLinejoin="round"
      />
      <circle cx="8" cy="8" r="1.9" stroke="currentColor" strokeWidth="1.2" fill="none" />
    </svg>
  ),
  folder: (
    <svg width="12" height="12" viewBox="0 0 16 16" fill="none">
      <path
        d="M1.5 4.2h4L7 5.7h7.5v7.6h-13z"
        stroke="currentColor"
        strokeWidth="1.2"
        fill="none"
        strokeLinejoin="round"
      />
    </svg>
  ),
  spark: (
    <svg width="11" height="11" viewBox="0 0 16 16" fill="none">
      <path d="M8 1.5l1.6 4.9L14.5 8l-4.9 1.6L8 14.5l-1.6-4.9L1.5 8l4.9-1.6z" fill="currentColor" />
    </svg>
  ),
  shield: (
    <svg width="13" height="13" viewBox="0 0 16 16" fill="none">
      <path
        d="M8 1.6l5 1.8v4.1c0 3.2-2.1 5.4-5 6.9-2.9-1.5-5-3.7-5-6.9V3.4z"
        stroke="currentColor"
        strokeWidth="1.2"
        fill="none"
      />
      <path
        d="M5.7 8.1l1.7 1.7 3-3.4"
        stroke="currentColor"
        strokeWidth="1.3"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  ),
  check: (
    <svg width="12" height="12" viewBox="0 0 16 16" fill="none">
      <path
        d="M3 8.5l3 3 7-7.5"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  ),
  close: (
    <svg width="10" height="10" viewBox="0 0 16 16" fill="none">
      <path
        d="M3.5 3.5l9 9M12.5 3.5l-9 9"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
      />
    </svg>
  ),
  undo: (
    <svg width="11" height="11" viewBox="0 0 16 16" fill="none">
      <path
        d="M3 6.5h7a3.5 3.5 0 0 1 0 7H6M3 6.5L6 3.5M3 6.5l3 3"
        stroke="currentColor"
        strokeWidth="1.4"
        strokeLinecap="round"
        strokeLinejoin="round"
        fill="none"
      />
    </svg>
  ),
};

export const GLYPH: Record<string, string> = {
  ImportDeclaration: "im",
  FunctionDeclaration: "fn",
  VariableDeclaration: "let",
  IfStatement: "if",
  ExpressionStatement: "()",
  ReturnStatement: "ret",
  ExportDeclaration: "ex",
};

export const SEV_LABEL: Record<Severity, string> = { high: "High", medium: "Medium", low: "Low" };
export const HL_COLOR: Record<Severity, string> = {
  high: "#f2604c",
  medium: "#e7a83e",
  low: "#6f8bc4",
};
export const HL_COLOR_A: Record<Severity, string> = {
  high: "rgba(242,96,76,0.5)",
  medium: "rgba(231,168,62,0.5)",
  low: "rgba(111,139,196,0.5)",
};
