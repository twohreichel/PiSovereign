# Hardware Setup Guide

This guide covers the hardware assembly and configuration for running PiSovereign on a Raspberry Pi 5 with Hailo-10H accelerator.

## Bill of Materials

### Required Components

| Component | Description | Notes |
|-----------|-------------|-------|
| Raspberry Pi 5 | 8GB RAM model recommended | 4GB works but limits concurrent users |
| Hailo-10H M.2 Module | 26 TOPS AI accelerator | M.2 Key M, 2280 form factor |
| Raspberry Pi M.2 HAT+ | Official M.2 adapter | Provides PCIe connection |
| microSD Card | 32GB+ Class 10/U1 | For OS and boot |
| Power Supply | 27W USB-C (5V/5A) | Official RPi 5 supply recommended |
| Active Cooler | Fan + heatsink | Required for sustained workloads |
| Case | With HAT+ support | Must accommodate M.2 HAT+ height |

### Optional Components

| Component | Description | Notes |
|-----------|-------------|-------|
| NVMe SSD | For faster storage | Can replace microSD for root |
| UPS HAT | Battery backup | For reliability |
| PoE+ HAT | Power over Ethernet | For rack deployments |

## Assembly Instructions

### Step 1: Prepare the Raspberry Pi 5

1. **Update firmware** (if using older Pi):
   ```bash
   sudo rpi-update
   ```

2. **Install heatsink/cooler** before mounting HAT
   - Apply thermal paste to CPU
   - Secure active cooler

### Step 2: Install M.2 HAT+

1. **Power off** the Raspberry Pi completely
2. **Connect ribbon cable** to PCIe slot on Pi 5
3. **Mount HAT+** using standoffs
4. **Secure ribbon cable** to HAT+ connector

### Step 3: Install Hailo-10H Module

1. **Insert M.2 module** at 30° angle into slot
2. **Press down** and secure with M.2 screw
3. **Verify seating** - module should sit flush

### Step 4: Case Assembly

1. Ensure adequate ventilation
2. Position fan for optimal airflow
3. Secure all cables

## Software Configuration

### Enable PCIe

Edit `/boot/firmware/config.txt`:

```ini
# Enable PCIe for M.2 HAT+
dtparam=pciex1
dtparam=pciex1_gen=3
```

Reboot after editing.

### Install Hailo Runtime

```bash
# Add Hailo repository
wget -qO - https://hailo.ai/keys/hailo.asc | sudo apt-key add -
echo "deb https://hailo.ai/deb stable main" | sudo tee /etc/apt/sources.list.d/hailo.list

# Install HailoRT
sudo apt update
sudo apt install hailort hailo-firmware
```

### Verify Installation

```bash
# Check PCIe device
lspci | grep -i hailo
# Expected: Hailo Technologies Ltd. Hailo-10H AI accelerator

# Check HailoRT
hailortcli version
# Expected: HailoRT version X.X.X

# Run benchmark
hailortcli benchmark --hef /path/to/model.hef
```

## Performance Tuning

### CPU Governor

Set performance mode for consistent latency:

```bash
echo "performance" | sudo tee /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor
```

Make persistent via `/etc/rc.local` or systemd service.

### Memory Configuration

Edit `/boot/firmware/config.txt`:

```ini
# Reduce GPU memory (headless server)
gpu_mem=16

# Enable 64-bit kernel (should be default)
arm_64bit=1
```

### Thermal Management

Configure fan curve in `/boot/firmware/config.txt`:

```ini
# Active cooling curve
dtoverlay=pwm-fan,gpiopin=14,temp0=50000,speed0=75
dtoverlay=pwm-fan,gpiopin=14,temp1=60000,speed1=125
dtoverlay=pwm-fan,gpiopin=14,temp2=70000,speed2=200
dtoverlay=pwm-fan,gpiopin=14,temp3=80000,speed3=255
```

### PCIe Tuning

For maximum Hailo performance:

```bash
# Check current link speed
lspci -vvv | grep -A20 Hailo | grep -E "LnkCap|LnkSta"

# Should show Gen3 x1 (8GT/s)
```

## Troubleshooting

### Device Not Detected

```bash
# Check physical connection
dmesg | grep -i hailo
dmesg | grep -i pcie

# Reseat M.2 module if no device found
```

### Thermal Throttling

```bash
# Monitor temperature
watch -n 1 vcgencmd measure_temp

# If >80°C under load:
# - Check cooler mounting
# - Improve case airflow
# - Consider larger heatsink
```

### Power Issues

Symptoms: Random crashes, USB devices disconnect

Solutions:
- Use official 27W power supply
- Disable USB power if not needed:
  ```bash
  echo '1-1' | sudo tee /sys/bus/usb/drivers/usb/unbind
  ```

### Performance Lower Than Expected

```bash
# Check CPU throttling
vcgencmd get_throttled
# 0x0 = no throttling

# Check memory pressure
free -h
cat /proc/meminfo | grep -E "MemAvailable|SwapFree"

# Check Hailo utilization
hailortcli monitor
```

## Benchmarks

Expected performance on Raspberry Pi 5 + Hailo-10H:

| Model | Tokens/Second | First Token Latency |
|-------|---------------|---------------------|
| LLaMA 3.2 1B | ~25 tok/s | ~200ms |
| LLaMA 3.2 3B | ~15 tok/s | ~400ms |
| Phi-2 2.7B | ~18 tok/s | ~350ms |

*Benchmarks vary by prompt length and quantization level.*

## See Also

- [Raspberry Pi 5 Documentation](https://www.raspberrypi.com/documentation/computers/raspberry-pi-5.html)
- [Hailo Developer Zone](https://hailo.ai/developer-zone/)
- [Deployment Guide](deployment.md)
- [Security Guide](security.md)
