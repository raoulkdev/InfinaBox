import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { TooltipProvider } from "@/components/ui/tooltip";
import { hydrateLayouts } from "@/lib/layout-store";
import "./index.css";

// Saved layouts are read before anything renders, so panels open where they were left.
void hydrateLayouts().then(() =>
  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <TooltipProvider>
      <App />
    </TooltipProvider>
  </React.StrictMode>,
),
);
