import { mount } from "svelte";
import SyncErrorFixture from "./SyncErrorFixture.svelte";
import "../../src/lib/styles/tokens.css";
import "../../src/lib/styles/app.css";

mount(SyncErrorFixture, { target: document.getElementById("app")! });
