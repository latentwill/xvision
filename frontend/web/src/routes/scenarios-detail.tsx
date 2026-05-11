import { useParams } from "react-router-dom";
import { Topbar } from "@/components/shell/Topbar";
import { Card } from "@/components/primitives/Card";

export function ScenariosDetailRoute() {
  const { id } = useParams<{ id: string }>();
  return (
    <>
      <Topbar title="Scenario detail" sub={id ?? "?"} />
      <Card>
        <p className="p-6 text-text-2">Detail view coming soon.</p>
      </Card>
    </>
  );
}
