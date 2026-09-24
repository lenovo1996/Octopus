import { mount } from "svelte";
import GraphFixture from "./GraphFixture.svelte";
import "../../src/lib/styles/app.css";

mount(GraphFixture, { target: document.getElementById("app")! });
