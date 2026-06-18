/// <reference types="vite/client" />

import type { CNshellApi } from "./shared/ipc";

declare global {
  interface Window {
    cnshell?: CNshellApi;
  }
}
