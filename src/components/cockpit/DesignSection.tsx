interface DesignSectionProps {
  projectPath: string | null;
}

// TODO: real GDD outline (docs/gdd/*.md) + editor, reusing MarkdownEditor
// and MarkdownPreview (both already built and working — see
// ViewportPanel's old implementation, now removed, for the load/dirty/save
// pattern to port over here, scoped to the docs/gdd folder instead of any
// arbitrary file).
export function DesignSection({ projectPath }: DesignSectionProps) {
  void projectPath;
  return (
    <div className="flex flex-1 items-center justify-center">
      <p className="text-sm text-muted-foreground">Design section — coming next.</p>
    </div>
  );
}
