# Owl Interaction Model

## Movement

- The owl starts perched in the bottom-right corner.
- During quiet periods it stays in either bottom corner, alternating occasionally to avoid looking static.
- Relevant events trigger an alert animation, then a short flight path that ends at the nearest bottom corner.
- The first overlay version never blocks desktop controls while roaming.

## Event policy

Only notify when a condition crosses a threshold, not on every polling interval.

| Signal | Example threshold | Owl response |
| --- | --- | --- |
| Battery | below 15% | alert callout and tired animation |
| Memory | above 90% | alert callout and concerned animation |
| Disk | below 10% free | perched warning and status card |
| Network | disconnected after active connection | short fly-in notification |

Each event has a cooldown and can be disabled individually.

## Approved actions

Actions are proposals, not automatic behavior. The owl presents the command, the reason, and its effects before executing it. Examples include opening the system monitor or running a user-defined maintenance script. The app must never invoke privileged commands, alter settings, or access new data sources without a separate grant.
