import { return_value, document_dir } from "ext:core/ops";

function returnValue(value) {
  return_value(globalThis.RuntimeExtension.taskId, JSON.stringify(value));
}

function documentDir() {
  return document_dir();
}

globalThis.RuntimeExtension = { returnValue, documentDir };
