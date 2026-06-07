import { useQuery } from "@tanstack/react-query";
import { listTools, toolKeys } from "@/api/tools";

export function ToolBadges({ tools }: { tools: string[] }) {
  const q = useQuery({ queryKey: toolKeys.all, queryFn: listTools });
  const descriptions = new Map(
    (q.data?.items ?? []).map((tool) => [tool.name, tool.description]),
  );
  if (tools.length === 0) {
    return <span className="text-text-3 text-[12px]">No tools</span>;
  }
  return (
    <div className="flex flex-wrap gap-1.5">
      {tools.map((tool) => (
        <span
          key={tool}
          title={descriptions.get(tool) ?? tool}
          className="rounded-sm border border-border bg-surface-card px-2 py-0.5 font-mono text-[11px] text-text-2"
        >
          {tool}
        </span>
      ))}
    </div>
  );
}
