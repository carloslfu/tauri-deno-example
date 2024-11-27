import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import CodeMirror from "@uiw/react-codemirror";
import { javascript } from "@codemirror/lang-javascript";
import { FaSpinner, FaStop, FaPlay } from "react-icons/fa";

import { nanoid } from "./lib/nanoid";

type Task = {
  id: string;
  code: string;
  state: "running" | "completed" | "error" | "stopped" | "stopping";
  result?: Record<string, any>;
  error?: string;
};

type InternalTask = {
  id: string;
  state: "running" | "completed" | "error" | "stopped";
  return_value?: string;
  error?: string;
};

const eventTarget = new EventTarget();

await listen<InternalTask>("task-state-changed", (event) => {
  eventTarget.dispatchEvent(
    new CustomEvent("task-state-changed", { detail: event.payload })
  );
});

const initialCode = `import * as cowsay from "https://esm.sh/cowsay@1.6.0"

console.log("-- taskId", RuntimeExtension.taskId)

// fetch a random user name from an example json api

// const randomUserId = Math.floor(Math.random() * 10) + 1

// const user = await fetch(\`https://jsonplaceholder.typicode.com/users/\${randomUserId}\`).then(r => r.json())

// const text = cowsay.say({
//   text: \`Hey \${user.name}! 🤠 (taskId: \${RuntimeExtension.taskId})\`,
// })

// console.log(text)

// access too a post

const post = await fetch("https://jsonplaceholder.typicode.com/posts/1").then(r => r.json())

console.log(post)

// write it to a file on the desktop by path

const desktopPath = await Deno.desktopPath()

console.log(desktopPath)

await Deno.writeTextFile(\`\${desktopPath}/post.json\`, JSON.stringify(post))

console.log("done")

RuntimeExtension.returnValue({ text, post })

// wait 5 seconds
await new Promise((resolve) => setTimeout(resolve, 5000))
`;

function App() {
  const [code, setCode] = useState(initialCode);
  const [result, setResult] = useState<Record<string, any> | undefined>();
  const [tasks, setTasks] = useState<Task[]>([]);

  const handleTaskStateChanged = useCallback((event: Event) => {
    const task = (event as CustomEvent<InternalTask>).detail;

    console.log("-- task state changed", task);

    let result: Record<string, any> | undefined;

    if (task.return_value) {
      result = JSON.parse(task.return_value);
    }

    setTasks((prev) =>
      prev.map((t) =>
        t.id === task.id
          ? {
              ...t,
              state: task.state,
              result,
              error: task.error,
            }
          : t
      )
    );

    if (task.state === "completed") {
      setResult(result);
    }
  }, []);

  useEffect(() => {
    eventTarget.addEventListener("task-state-changed", handleTaskStateChanged);

    return () => {
      eventTarget.removeEventListener(
        "task-state-changed",
        handleTaskStateChanged
      );
    };
  }, [handleTaskStateChanged]);

  const handleRunCode = async (codeToRun?: string) => {
    const newTaskId = nanoid();
    const newTask: Task = {
      id: newTaskId,
      code: codeToRun || code,
      state: "running",
    };

    try {
      setTasks((prev) => [...prev, newTask]);

      console.log("-- running code", newTaskId);

      await invoke("run_task", {
        taskId: newTaskId,
        code: codeToRun || code,
      });
    } catch (error) {
      console.error("Failed to run code:", error);
      setTasks((prev) =>
        prev.map((t) =>
          t.id === newTaskId ? { ...t, state: "error", result: { error } } : t
        )
      );
    }
  };

  const handleReplayTask = async (task: Task) => {
    setTasks((prev) =>
      prev.map((t) => (t.id === task.id ? { ...t, state: "running" } : t))
    );

    try {
      await invoke("run_task", {
        taskId: task.id,
        code: task.code,
      });
    } catch (error) {
      setTasks((prev) =>
        prev.map((t) =>
          t.id === task.id ? { ...t, state: "error", result: { error } } : t
        )
      );
    }
  };

  const handleStopTask = async (taskId: string) => {
    try {
      setTasks((prev) =>
        prev.map((t) => (t.id === taskId ? { ...t, state: "stopping" } : t))
      );
      await invoke("stop_task", { taskId });
    } catch (error) {
      console.error("Failed to stop task:", error);
      setTasks((prev) =>
        prev.map((t) =>
          t.id === taskId ? { ...t, state: "error", result: { error } } : t
        )
      );
    }
  };

  const handleClearCompletedTasks = () => {
    setTasks((prev) =>
      prev.filter((t) => t.state === "running" || t.state === "stopping")
    );
    invoke("clear_completed_tasks");
  };

  return (
    <div className="min-h-screen bg-gray-50 flex">
      <div className="flex-1 p-4">
        <div className="max-w-2xl mx-auto bg-white rounded-lg shadow-sm border border-gray-200">
          <div className="border-b border-gray-200 p-4">
            <h1 className="text-xl font-medium text-gray-900">Code Runner</h1>
          </div>

          <div className="p-4">
            <CodeMirror
              value={code}
              height="200px"
              extensions={[javascript({ jsx: true })]}
              onChange={(value) => setCode(value)}
            />
            <button
              onClick={() => handleRunCode()}
              className="mt-3 w-full bg-blue-500 hover:bg-blue-600 text-white font-medium py-2 px-4 rounded-md transition-colors"
            >
              Run Code
            </button>
            {tasks.length > 0 && (
              <div className="mt-4">
                <div className="flex justify-between items-center mb-2">
                  <h2 className="text-lg font-medium text-gray-900">Runs:</h2>
                  <button
                    onClick={handleClearCompletedTasks}
                    className="text-sm text-gray-500 hover:text-gray-700"
                  >
                    Clear Completed
                  </button>
                </div>
                <div className="space-y-2">
                  {tasks.map((task) => (
                    <div key={task.id} className="bg-gray-50 p-3 rounded-md">
                      <div className="flex items-center justify-between mb-2">
                        <div className="flex items-center gap-2">
                          <span className="font-mono text-sm">{task.id}</span>
                          {task.state === "running" && (
                            <>
                              <FaSpinner className="animate-spin text-blue-500" />
                              <button
                                onClick={() => handleStopTask(task.id)}
                                className="text-red-500 hover:text-red-600"
                              >
                                <FaStop />
                              </button>
                            </>
                          )}
                          {task.state === "stopping" && (
                            <>
                              <FaSpinner className="animate-spin text-yellow-500" />
                              <span className="text-yellow-500 text-sm">
                                Stopping...
                              </span>
                            </>
                          )}
                          {!["running", "stopping"].includes(task.state) && (
                            <button
                              onClick={() => handleReplayTask(task)}
                              className="text-green-500 hover:text-green-600"
                              title="Replay this task"
                            >
                              <FaPlay />
                            </button>
                          )}
                        </div>
                        <span
                          className={`text-sm ${
                            task.state === "completed"
                              ? "text-green-500"
                              : task.state === "error"
                              ? "text-red-500"
                              : task.state === "stopped"
                              ? "text-yellow-500"
                              : task.state === "stopping"
                              ? "text-yellow-500"
                              : "text-blue-500"
                          }`}
                        >
                          {task.state}
                        </span>
                      </div>
                      {task.error ? (
                        <div className="bg-red-50 border border-red-200 p-3 rounded-md font-mono text-sm overflow-auto max-h-64 whitespace-pre-wrap text-red-600">
                          {task.error}
                        </div>
                      ) : (
                        task.result && (
                          <div className="bg-gray-50 p-3 rounded-md font-mono text-sm overflow-auto max-h-64 whitespace-pre-wrap">
                            {task.result.text ||
                              ((task.state === "stopped" ||
                                task.state === "stopping") &&
                                "Task is being stopped...")}
                          </div>
                        )
                      )}
                    </div>
                  ))}
                </div>
              </div>
            )}

            {result && (
              <div className="mt-4">
                <h2 className="text-lg font-medium text-gray-900 mb-2">
                  Latest Result:
                </h2>
                <div className="bg-gray-50 p-3 rounded-md font-mono text-sm overflow-auto max-h-64 whitespace-pre-wrap">
                  {result.text}
                </div>
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

export default App;
