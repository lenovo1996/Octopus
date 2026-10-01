import { mount } from "svelte";
import BranchBadgeFixture from "./BranchBadgeFixture.svelte";
import "../../src/lib/styles/app.css";

mount(BranchBadgeFixture, { target: document.getElementById("app")! });
