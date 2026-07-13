/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_GOOGLE_CUSTOM_SEARCH_API_KEY?: string;
  readonly VITE_GOOGLE_CUSTOM_SEARCH_CX?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
