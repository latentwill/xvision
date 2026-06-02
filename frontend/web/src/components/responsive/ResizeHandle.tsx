import { useRef } from "react";

type Props = {
  onDelta: (delta: number) => void;
  hidden?: boolean;
};

export function ResizeHandle({ onDelta, hidden = false }: Props) {
  const lastX = useRef<number>(0);

  function onMouseDown(e: React.MouseEvent) {
    e.preventDefault();
    lastX.current = e.clientX;

    function onMove(ev: MouseEvent) {
      const delta = ev.clientX - lastX.current;
      lastX.current = ev.clientX;
      onDelta(delta);
    }

    function onUp() {
      document.removeEventListener("mousemove", onMove);
      document.removeEventListener("mouseup", onUp);
    }

    document.addEventListener("mousemove", onMove);
    document.addEventListener("mouseup", onUp);
  }

  return (
    <div
      className={`hidden lg:block relative self-stretch cursor-col-resize select-none z-10 ${hidden ? "pointer-events-none opacity-0" : ""}`}
      style={{ width: "4px" }}
      onMouseDown={onMouseDown}
    >
      <div
        className="absolute inset-y-0 group"
        style={{ left: "-4px", right: "-4px" }}
      >
        <div className="absolute inset-y-0 left-1/2 -translate-x-1/2 w-px bg-border opacity-50 group-hover:opacity-100 group-hover:w-[2px] transition-opacity" />
      </div>
    </div>
  );
}
