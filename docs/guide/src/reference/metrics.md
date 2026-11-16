# Health metrics

What a node reports in each health sample, every 30 seconds by default
(`--health-secs`). A metric that does not apply on a machine (a GPU on Linux, the rate
limits without a Claude login) is simply absent.

| Metric | Unit | What it is |
|---|---|---|
| `cpu.util_pct` | % | CPU utilisation across all cores since the last sample |
| `cpu.load_1m` | | One-minute load average |
| `mem.used_bytes`, `mem.total_bytes` | bytes | Memory in use and installed |
| `swap.used_bytes` | bytes | Swap in use |
| `disk.used_bytes`, `disk.total_bytes` | bytes | The disk holding the home directory |
| `net.rx_bytes`, `net.tx_bytes` | bytes | Received and sent on all interfaces since the last sample |
| `gpu.util_pct` | % | GPU device utilisation (macOS) |
| `gpu.mem_used_bytes` | bytes | GPU memory in use (macOS) |
| `claude.processes` | | Running Claude Code processes |
| `battery.charge_pct` | % | Battery charge; this and the other battery metrics only from a machine with a battery |
| `battery.health_pct` | % | What a full charge holds against what it held new (macOS's *Maximum Capacity*) |
| `battery.cycles` | | Charge cycles |
| `battery.charging` | 0 or 1 | Charging now |
| `battery.on_power` | 0 or 1 | Plugged in |
| `battery.temp_c` | °C | Battery temperature, where the machine reports it |
| `node.keep_awake` | 0 or 1 | Whether the daemon is holding the machine awake; only from a node that can |
| `node.keep_awake_enabled` | 0 or 1 | Whether keep-awake is switched on (`--keep-awake`, or the app's switch); only from a node that can hold |
| `usage.five_hour_pct` | % | Claude 5-hour window used |
| `usage.seven_day_pct` | % | Claude 7-day window used |
| `usage.seven_day_fable_pct` | % | Claude 7-day Fable window used |
| `usage.<window>_resets_s` | Unix seconds | When each of the three windows resets |
| `usage.age_s` | seconds | Age of the usage numbers: 0 when polled live (`--claude-usage`), otherwise the age of Claude Code's own cached copy |

The two keep-awake metrics are also sent on their own the moment either changes, so a
client sees a switch take effect within seconds.

The app charts CPU, memory, GPU, disk, battery charge and health, load, network in and
out (as MB/s, from the bytes since the previous sample and the time between the two) and
the three usage windows. The hub keeps every sample in its `health` table.
