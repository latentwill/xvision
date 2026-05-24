import React from "react";
import ReactDOM from "react-dom/client";
import "@fontsource/geist-sans/400.css";
import "@fontsource/geist-sans/500.css";
import "@fontsource/geist-sans/600.css";
import "@fontsource/geist-sans/700.css";
import "@fontsource/geist-sans/800.css";
import "@fontsource/geist-mono/400.css";
import "@fontsource/geist-mono/500.css";
import "@fontsource/geist-mono/600.css";
import "@fontsource/geist-mono/700.css";
import "./styles/tokens.css";
import "./styles/globals.css";
import { App } from "./App";
import { installBrowserLogging, logInfo, runtimeMode } from "./lib/logger";

installBrowserLogging();
logInfo("app", "app.boot", {
  mode: runtimeMode(),
  user_agent: navigator.userAgent,
});

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
