import React from "react";
import ReactDOM from "react-dom/client";
import "./styles.css";
import App from "./App";
import PromptWindow from "./pages/PromptWindow";

const isPrompt = window.location.hash.startsWith("#/prompt");

async function start() {
  if (import.meta.env.VITE_PASSVALET_E2E === "1") {
    await import("@wdio/tauri-plugin");
  }
  ReactDOM.createRoot(document.getElementById("root")!).render(
    <React.StrictMode>{isPrompt ? <PromptWindow /> : <App />}</React.StrictMode>,
  );
}

void start();
