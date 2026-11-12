import React from "react";
import ReactDOM from "react-dom/client";
import "./i18n";
import "./index.css";
// Brand-alike fonts for themes (bundled, offline).
import "@fontsource-variable/inter";
import "@fontsource-variable/noto-sans";
import "@fontsource-variable/figtree";
import "@fontsource-variable/mona-sans";
import "@fontsource-variable/jetbrains-mono";
import "@fontsource/lato/400.css";
import "@fontsource/lato/700.css";
import "@fontsource/lato/900.css";
import "@fontsource-variable/lora";
import "@fontsource-variable/roboto-flex";
import "@fontsource-variable/outfit";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import App from "./app/App";
import { ErrorBoundary, AppCrash } from "./components/ErrorBoundary";

const queryClient = new QueryClient({
  defaultOptions: { queries: { staleTime: 30_000, refetchOnWindowFocus: false } },
});

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <QueryClientProvider client={queryClient}>
      <ErrorBoundary fallback={(e) => <AppCrash error={e} />}>
        <App />
      </ErrorBoundary>
    </QueryClientProvider>
  </React.StrictMode>,
);
