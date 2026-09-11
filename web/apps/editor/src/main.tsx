import React from "react";
import { observeRenderedFonts } from "./typography/font-loading.js";
import { createRoot } from "react-dom/client";

import { ThemeRuntime } from "@open-office/ui";
import { App } from "./App.js";
import "./styles/index.css";
import "./styles/print.css";
import "./styles/presentation.css";

const container = document.getElementById("root");
if (!container) throw new Error("找不到 #root 挂载点");

observeRenderedFonts(container);

createRoot(container).render(
  <React.StrictMode>
    <ThemeRuntime initialTheme="office-light" storageKey="open-office.theme">
      <App />
    </ThemeRuntime>
  </React.StrictMode>,
);
