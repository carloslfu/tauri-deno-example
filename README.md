# Tauri with Deno example

![Screenshot](screenshot.png)

Example of using Tauri with `deno_runtime` to run multiple code tasks in parallel. This repo showcases parallel code execution, stopping tasks, and polling for results.

I use channels to stop tasks and hashmaps to store the return values and handles of the tasks.

Run it with:

```bash
pnpm tauri dev
```
