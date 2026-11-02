#!/usr/bin/env bash
# orb 启动时用 Amp 注入的环境变量生成本地密钥文件，给必须读文件的脚本用。
# 由 Mac 上的 ~/.config/amp-env 统一维护，各仓库里是同一份拷贝，别单独改。
#
# 用法（在 .agents/resume 里调用）：
#   .agents/amp-dotenv.sh                     生成仓库根目录的 .env.local（KEY="值" 格式）
#   .agents/amp-dotenv.sh <文件> raw          生成 KEY=值 格式（给 `. 文件` 或按行切分的读法用）
#
# 写哪些变量：AMPENV_PROJECT_NAMES（项目级）和 AMPENV_USER_NAMES（用户级）里的名字，
# 这两个清单由 Mac 上的 ~/.config/amp-env/push.py 在同步密钥时更新。
# 只在 orb 里运行；不打印任何值；不覆盖不是本脚本生成的文件。
set -euo pipefail

[ -n "${AMP_ORB:-}" ] || { echo "amp-dotenv：不在 orb 里，跳过"; exit 0; }

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out="${1:-$root/.env.local}"
mode="${2:-quoted}"
marker="# 由 .agents/amp-dotenv.sh 生成"

if [ -f "$out" ] && ! head -n 1 "$out" | grep -qF "$marker"; then
  echo "amp-dotenv：${out} 不是本脚本生成的，不覆盖"
  exit 0
fi

mkdir -p "$(dirname "$out")"
umask 077
tmp="$(mktemp "$(dirname "$out")/.amp-dotenv.XXXXXX")"
count=0
{
  echo "${marker}，orb 每次启动都会重写。要改值请用 amp secrets，别改这个文件。"
  IFS=',' read -r -a names <<<"${AMPENV_PROJECT_NAMES:-},${AMPENV_USER_NAMES:-}"
  seen=","
  for n in "${names[@]}"; do
    n="${n//[[:space:]]/}"
    [[ "$n" =~ ^[A-Za-z_][A-Za-z0-9_]*$ ]] || continue
    case "$seen" in *",$n,"*) continue ;; esac
    seen="$seen$n,"
    [ -n "${!n+x}" ] || continue
    v="${!n}"
    if [ "$mode" = raw ]; then
      # 按行读的格式放不下跨行值（比如证书），这类只走环境变量
      [[ "$v" == *$'\n'* ]] && continue
      printf '%s=%s\n' "$n" "$v"
    else
      v="${v//\\/\\\\}"
      v="${v//\"/\\\"}"
      v="${v//$'\n'/\\n}"
      printf '%s="%s"\n' "$n" "$v"
    fi
    count=$((count + 1))
  done
} >"$tmp"
mv -f "$tmp" "$out"

# 仓库内的生成文件加进本地忽略，避免被误提交
case "$out" in
  "$root"/*)
    rel="${out#"$root"/}"
    ex="$root/.git/info/exclude"
    if [ -d "$root/.git" ] && ! grep -qxF "$rel" "$ex" 2>/dev/null; then
      echo "$rel" >>"$ex"
    fi
    ;;
esac

echo "amp-dotenv：写入 ${out}，${count} 个变量"
