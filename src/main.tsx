import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./styles.css";
import { applyTheme, readPreferences } from "./preferences";
applyTheme(readPreferences().theme);
ReactDOM.createRoot(document.getElementById("root")!).render(<React.StrictMode><App /></React.StrictMode>);
