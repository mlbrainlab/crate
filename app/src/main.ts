import "./styles/fonts.css";
import "./styles/tokens.css";
import "./styles/base.css";
import { mount } from "svelte";
import App from "./App.svelte";
import { signalReady } from "./ready";

const app = mount(App, { target: document.getElementById("app")! });
signalReady();

export default app;
