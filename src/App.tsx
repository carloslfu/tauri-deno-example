import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import CodeMirror from "@uiw/react-codemirror";
import { javascript } from "@codemirror/lang-javascript";
import { FaSpinner, FaStop, FaPlay } from "react-icons/fa";

import { nanoid } from "./lib/nanoid";

interface Task {
  id: string;
  code: string;
  status: "running" | "completed" | "error";
  result?: Record<string, any>;
  error?: string;
}

const initialCode = `import * as cowsay from "https://esm.sh/cowsay@1.6.0"

console.log("-- taskId", RuntimeExtension.taskId)

// fetch a random user name from an example json api

const randomUserId = Math.floor(Math.random() * 10) + 1

const user = await fetch(\`https://jsonplaceholder.typicode.com/users/\${randomUserId}\`).then(r => r.json())

const text = cowsay.say({
  text: \`Hey \${user.name}! 🤠 (taskId: \${RuntimeExtension.taskId})\`,
})

console.log(text)

RuntimeExtension.returnValue({ text })

// wait 5 seconds
await new Promise((resolve) => setTimeout(resolve, 5000))
`;

function App() {
  const [code, setCode] = useState(initialCode);
  const [result, setResult] = useState<Record<string, any> | null>(null);
  const [tasks, setTasks] = useState<Task[]>([]);
  const [isPolling, setIsPolling] = useState(false);

  useEffect(() => {
    let interval: NodeJS.Timeout;

    if (isPolling) {
      interval = setInterval(async () => {
        const runningTasks = tasks.filter((t) => t.status === "running");

        for (const task of runningTasks) {
          try {
            console.log("-- polling", task.id);

            const taskState = await invoke("get_task_state", {
              taskId: task.id,
            });

            console.log("-- task state", task.id, taskState);

            if ((taskState as any).state === "completed") {
              const returnValue = (taskState as any).return_value;
              let parsedResult: Record<string, any>;
              try {
                parsedResult = JSON.parse(returnValue);
              } catch (error) {
                parsedResult = {
                  error,
                  result: returnValue,
                };
              }

              setTasks((prev) =>
                prev.map((t) =>
                  t.id === task.id
                    ? { ...t, status: "completed", result: parsedResult }
                    : t
                )
              );

              if (task.id === tasks[tasks.length - 1]?.id) {
                setResult(parsedResult);
              }
            } else if ((taskState as any).state === "error") {
              setTasks((prev) =>
                prev.map((t) =>
                  t.id === task.id
                    ? {
                        ...t,
                        status: "error",
                        error: (taskState as any).error,
                        result: { error: (taskState as any).error },
                      }
                    : t
                )
              );
            }
          } catch (error) {
            if ((error as string) === "Task not found") {
              setTasks((prev) =>
                prev.map((t) =>
                  t.id === task.id
                    ? { ...t, status: "error", result: { error } }
                    : t
                )
              );
            } else {
              console.log("Error polling task:", error);
            }
          }
        }

        if (runningTasks.length === 0) {
          setIsPolling(false);
        }
      }, 2000);
    }

    return () => clearInterval(interval);
  }, [isPolling, tasks]);

  const handleRunCode = async (codeToRun?: string) => {
    const newTaskId = nanoid();
    const newTask: Task = {
      id: newTaskId,
      code: codeToRun || code,
      status: "running",
    };

    try {
      setTasks((prev) => [...prev, newTask]);
      setIsPolling(true);

      console.log("-- running code", newTaskId);

      await invoke("run_code", {
        taskId: newTaskId,
        code: codeToRun || code,
      });
    } catch (error) {
      console.error("Failed to run code:", error);
      setTasks((prev) =>
        prev.map((t) =>
          t.id === newTaskId ? { ...t, status: "error", result: { error } } : t
        )
      );
    }
  };

  const handleReplayTask = (taskCode: string) => {
    handleRunCode(taskCode);
  };

  const handleStopTask = async (taskId: string) => {
    try {
      await invoke("stop_code", { taskId });
      setTasks((prev) =>
        prev.map((t) =>
          t.id === taskId
            ? {
                ...t,
                status: "completed",
                result: { cancelled: true, message: "Task cancelled" },
              }
            : t
        )
      );
    } catch (error) {
      console.error("Failed to stop task:", error);
    }
  };

  const handleClearCompletedTasks = () => {
    setTasks((prev) => prev.filter((t) => t.status === "running"));
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
                          {task.status === "running" ? (
                            <>
                              <FaSpinner className="animate-spin text-blue-500" />
                              <button
                                onClick={() => handleStopTask(task.id)}
                                className="text-red-500 hover:text-red-600"
                              >
                                <FaStop />
                              </button>
                            </>
                          ) : (
                            <button
                              onClick={() => handleReplayTask(task.code)}
                              className="text-green-500 hover:text-green-600"
                              title="Replay this task"
                            >
                              <FaPlay />
                            </button>
                          )}
                        </div>
                        <span
                          className={`text-sm ${
                            task.status === "completed"
                              ? "text-green-500"
                              : task.status === "error"
                              ? "text-red-500"
                              : "text-blue-500"
                          }`}
                        >
                          {task.status}
                        </span>
                      </div>
                      {task.result && (
                        <div className="bg-gray-50 p-3 rounded-md font-mono text-sm overflow-auto max-h-64 whitespace-pre-wrap">
                          {task.error || task.result.text}
                        </div>
                      )}
                    </div>
                  ))}
                </div>
              </div>
            )}

            {result !== null && (
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
