import React from "react";
import ReactDOM from "react-dom/client";
import "./i18n";
import "./index.css";
// Theme fonts (bundled, offline) — latin + cyrillic subsets only.
import "./fonts.css";
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
