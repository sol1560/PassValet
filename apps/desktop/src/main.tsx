import React from "react";
import ReactDOM from "react-dom/client";
import "./styles.css";
import App from "./App";
import PromptWindow from "./pages/PromptWindow";

const isPrompt = window.location.hash.startsWith("#/prompt");

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>{isPrompt ? <PromptWindow /> : <App />}</React.StrictMode>,
);
