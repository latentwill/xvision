import { Link } from "react-router-dom";

export function NotFoundRoute() {
  return (
    <div className="px-6 py-16 text-center">
      <p className="mb-1 font-mono text-[11px] uppercase tracking-[0.15em] text-text-4">
        404
      </p>
      <h1 className="mb-3 text-[28px] font-semibold tracking-tight text-text">
        Page not found
      </h1>
      <p className="mb-6 text-[13px] text-text-3">
        The page you requested doesn't exist.
      </p>
      <Link
        to="/"
        className="inline-flex items-center gap-2 rounded border border-border px-4 py-2 text-[13px] text-text hover:border-text-3 hover:text-text transition-colors"
      >
        Go home
      </Link>
    </div>
  );
}
