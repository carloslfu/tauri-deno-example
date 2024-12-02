# Run Deno tasks with Tauri

![Screenshot](screenshot.png)

This is an example of using Tauri and `deno_runtime` to run multiple Deno tasks in parallel. This repo showcases parallel code execution, stopping tasks, handling permissions, and getting results.

It uses channels to stop tasks and hashmaps to store the return values and thread handles of the tasks. The Tauri <> Rust communication is done through Tauri events and commands.

If there are pending permission requests it could block some tasks that need permissions due to a `deno_runtime` limitation. See this Deno issue for more details: https://github.com/denoland/deno/issues/27160.

Run it with:

```bash
pnpm tauri dev
```
