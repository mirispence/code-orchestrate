import { EmptyState } from "@/components/EmptyState";
import { Sidebar } from "@/components/Sidebar";

// Add Workspace and Suggestions are wired up by later tickets.
const notYet = () => {};

export default function App() {
  return (
    <div className="flex h-full">
      <Sidebar onAddWorkspace={notYet} onOpenSuggestions={notYet} />
      <main className="min-w-0 flex-1">
        <EmptyState onAddWorkspace={notYet} onOpenSuggestions={notYet} />
      </main>
    </div>
  );
}
