# Reminder System

PiSovereign includes a proactive reminder system that helps you stay on top of appointments, tasks, and custom reminders. The system integrates with CalDAV calendars and provides beautiful German-language notifications via WhatsApp or Signal.

## Features

- **Calendar Integration**: Automatically creates reminders from CalDAV events
- **Custom Reminders**: Create personal reminders with natural language
- **Smart Notifications**: Beautiful formatted messages with emoji and key information
- **Location Support**: Google Maps links and ÖPNV transit connections for location-based events
- **Snooze Management**: Snooze reminders up to 5 times (configurable)
- **Morning Briefing**: Daily summary of your upcoming appointments

## Natural Language Commands

### Creating Reminders

```
"Erinnere mich morgen um 10 Uhr an den Arzttermin"
"Remind me tomorrow at 3pm to call mom"
"Erinnere mich in 2 Stunden an die Wäsche"
```

### Listing Reminders

```
"Zeige meine Erinnerungen"
"Welche Termine habe ich heute?"
"Liste alle aktiven Erinnerungen"
```

### Snoozing Reminders

```
"Erinnere mich nochmal in 15 Minuten"
"Snooze für eine Stunde"
```

### Acknowledging Reminders

```
"Ok, danke!"
"Erledigt"
```

### Deleting Reminders

```
"Lösche die Erinnerung zum Arzttermin"
```

## Transit Connections

When you have an appointment at a specific location, PiSovereign can automatically include ÖPNV (public transit) connections in your reminder:

```
📅 **Meeting mit Hans**
📍 Alexanderplatz 1, Berlin
🕒 Morgen um 14:00 Uhr

🚇 **So kommst du hin:**
🚌 Bus 200 → S-Bahn S5 → U-Bahn U2
   Abfahrt: 13:22 (38 min)
   Ankunft: 14:00

🗺️ [Auf Google Maps öffnen](https://www.google.com/maps/...)
```

### Searching Transit Routes

You can also search for transit connections directly:

```
"Wie komme ich zum Hauptbahnhof?"
"ÖPNV Verbindung nach Alexanderplatz"
```

## Configuration

Add the following sections to your `config.toml`:

### Transit Configuration

```toml
[transit]
# Include transit info in location-based reminders
include_in_reminders = true

# Your home location for route calculations
home_location = { latitude = 52.52, longitude = 13.405 }

# Transport modes to include
products_bus = true
products_suburban = true    # S-Bahn
products_subway = true      # U-Bahn
products_tram = true
products_regional = true    # RB/RE
products_national = false   # ICE/IC
```

### Reminder Configuration

```toml
[reminder]
# Maximum number of snoozes per reminder (default: 5)
max_snooze = 5

# Default snooze duration in minutes (default: 15)
default_snooze_minutes = 15

# How far in advance to create reminders from CalDAV events
caldav_reminder_lead_time_minutes = 30

# Interval for checking due reminders (seconds)
check_interval_secs = 60

# CalDAV sync interval (minutes)
caldav_sync_interval_minutes = 15

# Morning briefing settings
morning_briefing_time = "07:00"
morning_briefing_enabled = true
```

### CalDAV Configuration

For calendar integration, you need a CalDAV server (like Baikal, Radicale, or Nextcloud):

```toml
[caldav]
server_url = "https://cal.example.com/dav.php"
username = "your-username"
password = "your-password"
calendar_path = "/calendars/user/default"
```

## Reminder Sources

Reminders can come from two sources:

1. **CalDAV Events**: Automatically synced from your calendar
2. **Custom Reminders**: Created via natural language commands

CalDAV events include the original event details (title, time, location) while custom reminders are more flexible and can include any text.

## Notification Format

Reminders are formatted as beautiful German messages with:

- **Bold headers** for event titles
- **Emoji prefixes** for quick scanning (📅 📍 🕒)
- **Time formatting** relative to now ("in 30 Minuten")
- **Location links** to Google Maps
- **Transit info** for getting there

Example reminder notification:

```
📅 **Zahnarzt Dr. Müller**
📍 Friedrichstraße 123, Berlin
🕒 Heute um 15:00 (in 2 Stunden)

🗺️ Auf Google Maps öffnen
```

## Morning Briefing

When enabled, you receive a daily summary at the configured time (default 7:00 AM):

```
☀️ **Guten Morgen!**

📅 **Heute hast du 3 Termine:**

1. 09:00 - Team Meeting (Büro)
2. 12:30 - Mittagessen mit Lisa (Restaurant Mitte)
3. 16:00 - Arzttermin (Praxis Dr. Schmidt)

🌤️ Wetter: 18°C, leicht bewölkt

📋 **Offene Erinnerungen:**
- Geburtstagskarte für Mama kaufen
- Wäsche abholen
```

## Snooze Limits

Each reminder can be snoozed up to `max_snooze` times (default: 5). After that, the system will indicate that no more snoozes are available:

```
⏰ Diese Erinnerung wurde bereits 5x verschoben.
Bitte bestätige oder lösche sie.
```

## Status Tracking

Reminders go through these states:

- **Pending**: Waiting for the remind time
- **Sent**: Notification was delivered
- **Acknowledged**: User confirmed receipt
- **Snoozed**: User requested a later reminder
- **Deleted**: User removed the reminder

You can list reminders filtered by status using commands like "zeige alle erledigten Erinnerungen".
