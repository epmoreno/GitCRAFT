import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "./ui/tokens.css";
import "./i18n";
import { App } from "./app/App";
import { os } from "./app/platform";

// CSS that depends on the system (the macOS title bar) reads this.
document.documentElement.dataset.os = os();

const root = document.getElementById("root");
if (!root) throw new Error("#root element missing");

createRoot(root).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
