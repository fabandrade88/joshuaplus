import { mount } from "svelte";
import "@fontsource-variable/jetbrains-mono/wght.css";
import "./app.css";
import App from "./App.svelte";

const target = document.getElementById("app");
if (!target) throw new Error("#app missing");

export default mount(App, { target });
