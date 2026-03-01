# Sampling

Each monitored resource has it's own sampling config. The resource monitors are only if their respective [component](./Components.md) is being used.

The config is composed of an `update_interval` in milliseconds (should be bigger than 100ms), and the `sampling_window`,
which corresponds to the number of samples stored in the memory (should be at least 1) and showed on Run charts.

**Por defecto** el intervalo de actualización es de **5 segundos** (5000 ms).

## Example

```ron
(
    cpu: (
        update_interval: 5000,  // 5 segundos
        sampling_window: 60,
    ),
    mem: (
        update_interval: 5000,
        sampling_window: 30,
    ),
    net: (
        update_interval: 5000,
        sampling_window: 30,
    ),
    disk: (
        update_interval: 5000,
        sampling_window: 20,
    ),
    gpu: (
        update_interval: 5000,
        sampling_window: 30,
    ),
)
```