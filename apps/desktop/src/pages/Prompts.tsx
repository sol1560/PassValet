import type { PendingPrompt } from "../lib/types";
import { PromptCard } from "./PromptWindow";

export default function Prompts({ prompts, onChanged }: { prompts: PendingPrompt[]; onChanged: () => void }) {
  return (
    <div>
      <div className="page-head">
        <div>
          <h1>待确认请求</h1>
          <p className="muted" style={{ margin: 0 }}>agent 在等待你的授权。</p>
        </div>
      </div>
      {prompts.length === 0 && <p className="muted">没有待处理请求。</p>}
      <div className="col">
        {prompts.map((p) => (
          <div className="card" key={p.id}>
            <PromptCard prompt={p} onDone={onChanged} />
          </div>
        ))}
      </div>
    </div>
  );
}
