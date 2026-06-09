import ReactDOM from "react-dom/client";
import App from "./App";
// Bundle Cascadia Code (the design's primary mono) so the mono face is identical
// on every Win11 machine, not dependent on a local install. UI font (Segoe UI
// Variable) ships with Windows. Weights match the design (400/500/600/700).
import "@fontsource/cascadia-code/400.css";
import "@fontsource/cascadia-code/500.css";
import "@fontsource/cascadia-code/600.css";
import "@fontsource/cascadia-code/700.css";
import "./styles/tokens.css";
import "./styles/app.css";

// No StrictMode: the prototype rendered once; this avoids dev double-invoking the
// load-scroll effect / pulse timers and keeps behavior identical to the reference.
ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(<App />);
