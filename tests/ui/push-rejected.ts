import { mount } from "svelte";
import PushRejectedFixture from "./PushRejectedFixture.svelte";
import "../../src/lib/styles/app.css";

mount(PushRejectedFixture, { target: document.getElementById("app")! });
