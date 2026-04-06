type ViewLoadingFallbackProps = {
  label: string;
};

export function ViewLoadingFallback({ label }: ViewLoadingFallbackProps) {
  return (
    <div className="flex min-h-0 flex-1 items-center justify-center px-6 py-8">
      <p className="text-sm text-muted-foreground">{label}</p>
    </div>
  );
}
