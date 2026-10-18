import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import WebApp from "./WebApp";
import "../styles.css";
import "./web.css";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <WebApp />
  </StrictMode>,
);
