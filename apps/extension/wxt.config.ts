import { defineConfig } from "wxt";

export default defineConfig({
  srcDir: ".",
  outDir: ".output",
  manifest: {
    name: "PassValet",
    description: "Lets the PassValet desktop app collect and rotate API keys inside your signed-in browser tabs.",
    version: "0.1.0",
    permissions: ["debugger", "tabs", "tabGroups", "nativeMessaging", "storage", "alarms"],
    host_permissions: ["<all_urls>"],
    icons: { "32": "icon-32.png", "128": "icon-128.png" },
    action: { default_title: "PassValet" },
  },
});
