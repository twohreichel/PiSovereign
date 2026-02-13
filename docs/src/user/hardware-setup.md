# Hardware Setup

> 🔧 Hardware assembly guide for Raspberry Pi 5 with Hailo-10H AI HAT+

This guide covers the physical hardware setup. For software installation,
see the [Docker Setup](./docker-setup.md) guide.

## Required Components

| Component | Recommended Model | Notes |
|-----------|-------------------|-------|
| **Raspberry Pi 5** | 8 GB RAM variant | 4 GB works but limits concurrent operations |
| **Hailo AI HAT+ 2** | Hailo-10H (26 TOPS) | Mounts via 40-pin GPIO + PCIe |
| **Power Supply** | Official 27W USB-C | Required for HAT+ power delivery |
| **Cooling** | Active Cooler for Pi 5 | Essential for sustained AI inference |
| **Storage** | NVMe SSD (256 GB+) | Via Hailo HAT+ PCIe or separate HAT |
| **MicroSD Card** | 32 GB+ Class 10 | For boot (if not using NVMe boot) |
| **Case** | Official Pi 5 Case (tall) | Must accommodate HAT+ height |

## Assembly Instructions

> ⚠️ **Important**: Always work on a static-free surface and handle boards by edges only.

### Step 1: Prepare the Raspberry Pi

1. Unbox the Raspberry Pi 5
2. Attach the Active Cooler:
   - Remove the protective film from the thermal pad
   - Align with the CPU and press firmly
   - Connect the 4-pin fan connector to the FAN header

### Step 2: Install the Hailo AI HAT+

1. Locate the 40-pin GPIO header on the Pi
2. Align the Hailo HAT+ with the GPIO pins
3. Gently press down until fully seated (approximately 3mm gap)
4. Connect the PCIe FPC cable:
   - Open the Pi 5's PCIe connector latch
   - Insert the flat cable (contacts facing down)
   - Close the latch to secure

### Step 3: Install Storage (Optional NVMe)

If using the Hailo HAT+ built-in M.2 slot:

1. Insert NVMe SSD into M.2 slot (M key, 2242/2280)
2. Secure with the provided screw

### Step 4: Enclose and Power

1. Place assembly in case
2. Connect Ethernet cable (recommended over WiFi for production)
3. Connect power supply

## OS Installation

### Flash Raspberry Pi OS

1. Install [Raspberry Pi Imager](https://www.raspberrypi.com/software/) on your computer
2. **Choose Device**: Raspberry Pi 5
3. **Choose OS**: Raspberry Pi OS Lite (64-bit)
4. Click **Edit Settings**:
   - Set hostname: `pisovereign`
   - Set username and strong password
   - Enable SSH with public-key authentication
   - Set your timezone

5. Flash to SD card / NVMe

### First Boot

```bash
# SSH into the Pi
ssh pi@pisovereign.local

# Update system
sudo apt update && sudo apt full-upgrade -y

# Install Docker (required for PiSovereign)
curl -fsSL https://get.docker.com | sudo sh
sudo usermod -aG docker $USER

# Log out and back in for group change
exit
```

### Configure Boot (Optional NVMe)

```bash
sudo raspi-config
```

- **Advanced Options** → **Boot Order** → **NVMe/USB Boot**

## Next Steps

Once hardware is assembled and Docker is installed, proceed to
the [Docker Setup](./docker-setup.md) guide for PiSovereign deployment.
