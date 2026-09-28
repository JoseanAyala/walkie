import { mount } from "svelte";
import "@/lib/theme.css";
import { followTheme } from "@/lib/theme";
import App from "./App.svelte";

followTheme();
mount(App, { target: document.body });
