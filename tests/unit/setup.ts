import "@testing-library/jest-dom/vitest";
import { afterEach } from "vitest";
import { cleanup } from "@testing-library/react";

// Without vitest globals, Testing Library can't self-register its cleanup.
afterEach(cleanup);
