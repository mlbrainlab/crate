import "./styles/fonts.css";
import "./styles/tokens.css";
import "./styles/base.css";
import { mount } from "svelte";
import App from "./App.svelte";
import MiniView from "./lib/MiniView.svelte";
import { signalReady } from "./ready";

const view = new URLSearchParams(location.search).get("view");
const app = mount(view === "mini" ? MiniView : App, { target: document.getElementById("app")! });
signalReady();

export default app;
