const $ = (id: string) => document.getElementById(id)!;

async function refresh() {
  const res = (await chrome.runtime.sendMessage({ type: "pv:status" }).catch(() => null)) as
    | { connected: boolean; runs: string[] }
    | null;
  const connected = !!res?.connected;
  $("dot").className = "dot " + (connected ? "ok" : "bad");
  $("status").textContent = connected ? "已连接桌面 app" : "未连接";
  $("help").style.display = connected ? "none" : "block";
  $("extid").textContent = chrome.runtime.id;
  const runs = res?.runs ?? [];
  $("runs").textContent = runs.length ? `正在采集：${runs.length} 个任务` : "空闲";
  $("abort").style.display = runs.length ? "block" : "none";
}

$("reconnect").addEventListener("click", async () => {
  await chrome.runtime.sendMessage({ type: "pv:reconnect" });
  setTimeout(refresh, 500);
});
$("abort").addEventListener("click", async () => {
  await chrome.runtime.sendMessage({ type: "pv:abort" });
  setTimeout(refresh, 300);
});

refresh();
setInterval(refresh, 1500);
