import { mount } from "svelte";
import Pill from "./Pill.svelte";
import "@fontsource-variable/inter";
import "./app.css";

const target = document.getElementById("pill");
if (!target) throw new Error("no #pill element");

export default mount(Pill, { target });
