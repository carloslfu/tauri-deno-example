# Run Deno tasks with Tauri

![Screenshot](screenshot.png)

Example of using Tauri with `deno_runtime` to run multiple tasks in parallel. This repo showcases parallel code execution, stopping tasks, handling permissions, and getting results.

This repo uses channels to stop tasks and hashmaps to store the return values and handles of the tasks. The Tauri <> Rust communication is done through Tauri events and commands.

If there are pending permission requests, it could block some tasks that might also need permissions from running due to a `deno_runtime` limitation. See this issue for more details: https://github.com/denoland/deno/issues/27160.

Run it with:

```bash
pnpm tauri dev
```
