import "@fontsource-variable/manrope";
import "./styles/global.css";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import { UpdateToast } from "./components/UpdateToast";

// The same page also serves the small update popup window (index.html?view=toast).
const isToast = new URLSearchParams(window.location.search).get("view") === "toast";

createRoot(document.getElementById("root")!).render(<StrictMode>{isToast ? <UpdateToast /> : <App />}</StrictMode>);
