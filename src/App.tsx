import { Button } from "@/components/ui/button";

function App() {
  return (
    <div className="flex h-screen w-screen flex-col items-center justify-center gap-6 bg-background text-foreground">
      <h1 className="text-4xl font-semibold tracking-tight">InfinaBox</h1>
      <Button>Get Started</Button>
    </div>
  );
}

export default App;
