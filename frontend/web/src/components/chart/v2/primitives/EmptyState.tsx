type Props = {
  title: string;
  message: string;
};

export function EmptyState({ title, message }: Props) {
  return (
    <div className="flex flex-col items-center justify-center min-h-[200px] px-6 py-8 border border-dashed border-border rounded-card text-center">
      <p className="text-[14px] font-medium text-text-2 mb-1">{title}</p>
      <p className="text-[12px] text-text-3 max-w-sm">{message}</p>
    </div>
  );
}
