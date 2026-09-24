import { mount } from "svelte";
import App from "./app/App.svelte";
import "./lib/styles/tokens.css";
import "./lib/styles/app.css";

const el = document.getElementById("app");
if (!el) throw new Error("Missing #app element");
mount(App, { target: el });
