import { mount } from "svelte";
import BitbucketAuthFixture from "./BitbucketAuthFixture.svelte";
import "../../src/lib/styles/tokens.css";
import "../../src/lib/styles/app.css";

mount(BitbucketAuthFixture, { target: document.getElementById("app")! });
