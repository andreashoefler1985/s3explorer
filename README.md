<div align="center">
  <br/>
  <h1>🪣 S3 Explorer</h1>
  <p><strong>Ein nativer GTK4-S3-Browser für Linux — geschrieben in Rust.</strong></p>

  <p>
    <img src="https://img.shields.io/badge/Rust-1.85%2B-dea584?logo=rust" alt="Rust"/>
    <img src="https://img.shields.io/badge/GTK-4.12-7f5f9c?logo=gtk" alt="GTK4"/>
    <img src="https://img.shields.io/github/license/andreashoefler1985/r2" alt="License"/>
    <img src="https://img.shields.io/badge/status-alpha-yellow" alt="Status"/>
  </p>

  <br/>
</div>

---

**r2** ist ein nativer S3-Client für Linux mit einer GTK4-Oberfläche. Er bietet eine **Zwei-Pane-Ansicht** (inspiriert von Midnight Commander / Far Manager) zum gleichzeitigen Durchsuchen und Verwalten von zwei S3-Buckets oder -Ordnern. Der Fokus liegt auf Geschwindigkeit, Sicherheit und einer nahtlosen Desktop-Integration.

> ⚠️ **Alpha-Phase** – Die Kernfunktionen sind implementiert, die UI wird kontinuierlich weiterentwickelt.

---

## ✨ Features

### 🔍 Dual-Pane-Browser
Zwei unabhängige Panels nebeneinander – jedes mit eigenem Profil, Bucket und Prefix. Perfekt zum Vergleichen, Verschieben oder Kopieren zwischen Buckets.

### 🔐 Sicheres Credential-Management
- Speicherung über **libsecret** (GNOME Keyring / KDE Wallet)
- **Verschlüsselter Datei-Fallback** (AES-256-GCM) für Systeme ohne Keyring
- Unterstützung für mehrere Profile mit verschiedenen Endpunkten und Regionen

### ⚡ Leistungsstarker Transfer-Engine
- **Multipart-Upload/-Download** für große Dateien (ab 100 MB)
- **Pause/Resume/Cancel** für laufende Transfers
- **Automatische Wiederholungsversuche** bei Fehlern (konfigurierbar)
- **Concurrency-Limit** für parallele Transfers
- **S3↔S3-Kopien** (serverseitig oder via Download/Upload bei verschiedenen Endpunkten)
- **Echtzeit-Fortschritt** mit Geschwindigkeits- und ETA-Anzeige

### 🗂️ Metadaten-Cache
- **SQLite-basierter Cache** für Bucket-Listen und Object-Listings
- Konfigurierbare TTL – reduziert API-Aufrufe und beschleunigt die Navigation
- Automatische Cache-Aktualisierung bei Änderungen

### 🖥️ Native GTK4-Oberfläche
- **Zwei-Pane-Ansicht** mit synchronisierter Navigation
- **Drag & Drop** zwischen Panels und vom Dateimanager
- **Kontextmenüs** für Dateioperationen (Download, Löschen, Umbenennen, Eigenschaften)
- **Transfer-Queue** mit Fortschrittsbalken und Statusanzeige
- **Profile-Manager** zum Hinzufügen/Bearbeiten von S3-Endpunkten
- **Tastaturkürzel** für effiziente Bedienung

---

## 🚀 Installation

### Voraussetzungen
- **Linux** mit GTK4 (≥ 4.12) und einem D-Bus Session Bus
- **Rust** 1.85+ und Cargo

### Aus dem Quellcode bauen

```bash
# Repository klonen
git clone https://github.com/andreashoefler1985/r2.git
cd r2

# Abhängigkeiten installieren (Ubuntu/Debian)
sudo apt install libgtk-4-dev libdbus-1-dev pkg-config

# Bauen
cargo build --release

# Ausführen
./target/release/r2
```

### Als Debian-Paket bauen

```bash
./scripts/build-deb.sh
sudo dpkg -i target/debian/r2_*.deb
```

---

## 🎮 Verwendung

### Profile einrichten
1. Starte r2
2. Öffne den Profile-Manager (`Strg+P`)
3. Füge einen neuen S3-Endpunkt hinzu:
   - **Name**: Ein beliebiger Anzeigename
   - **Endpoint**: z. B. `https://s3.eu-central-1.amazonaws.com` (oder S3-kompatibel wie MinIO, Backblaze B2, Cloudflare R2)
   - **Region**: z. B. `eu-central-1`
   - **Access Key ID** und **Secret Access Key**

### Navigation
- **Panel wechseln**: `Tab`
- **Bucket auswählen**: Dropdown oben im Panel
- **Ordner öffnen**: Doppelklick oder `Enter`
- **Zurück**: `Alt+Links` oder Breadcrumb-Klick
- **Aktuellen Pfad kopieren**: `Strg+C`

### Dateioperationen
- **Download**: Rechtsklick → "Herunterladen" oder Drag & Drop auf lokalen Ordner
- **Upload**: Datei aus dem Dateimanager in ein Panel ziehen
- **Zwischen Panels kopieren**: Objekt von Panel A nach Panel B ziehen
- **Löschen**: `Entf` oder Rechtsklick → "Löschen"
- **Umbenennen**: `F2` oder Rechtsklick → "Umbenennen"
- **Eigenschaften**: `Strg+I` oder Rechtsklick → "Eigenschaften"

### Transfers verwalten
- **Transfer-Queue anzeigen**: `Strg+T`
- **Transfer pausieren**: Klick auf Pause-Button
- **Transfer fortsetzen**: Klick auf Resume-Button
- **Transfer abbrechen**: Klick auf Cancel-Button

---

## 🏗️ Architektur

```
r2/
├── r2-core/          # Kernbibliothek (Business-Logik)
│   ├── src/
│   │   ├── s3_client/     # S3-Client-Trait + AWS SDK-Implementierung
│   │   ├── credentials/   # Credential-Storage (libsecret + verschlüsselte Datei)
│   │   ├── cache/         # Metadaten-Cache (SQLite)
│   │   ├── transfer/      # Transfer-Engine (Tokio-basiert, Multipart)
│   │   ├── events.rs      # Event-Typen für UI-Kommunikation
│   │   └── error.rs       # Einheitliches Error-Handling
│   └── tests/
│
├── r2-ui/            # GTK4-Oberfläche
│   ├── src/
│   │   ├── app.rs          # Haupt-App (Fenster, Layout)
│   │   ├── pane.rs         # Einzelnes Browser-Panel
│   │   ├── profile_manager.rs  # Profile-Manager-Dialog
│   │   ├── widgets.rs      # Custom GTK4-Widgets
│   │   └── dialogs/        # Dialoge (Bestätigung, Umbenennen, Eigenschaften)
│   └── ...
│
├── scripts/          # Build- und Deployment-Skripte
├── resources/        # Desktop-Integration (.desktop-Datei)
└── adr/              # Architecture Decision Records
```

### Technologie-Stack

| Komponente | Technologie |
|---|---|
| **Sprache** | Rust (Edition 2021) |
| **UI** | GTK4 (via `gtk4-rs`) |
| **Async Runtime** | Tokio (Multi-Threaded) |
| **S3-API** | AWS SDK for Rust (`aws-sdk-s3`) |
| **Credential Storage** | libsecret (D-Bus) + AES-256-GCM File Fallback |
| **Cache** | SQLite (via `rusqlite`) |
| **Transfer** | Eigenes Tokio-basiertes Multipart-Engine |
| **Build** | Cargo Workspace |

---

## 🧪 Tests

```bash
# Unit-Tests
cargo test -p r2-core

# Integrationstests
cargo test -p r2-core --test integration_test

# Alle Tests
cargo test
```

---

## 🗺️ Roadmap

- [x] Dual-Pane-Architektur
- [x] S3-Client mit AWS SDK
- [x] Credential-Storage (libsecret + verschlüsselter Fallback)
- [x] Metadaten-Cache (SQLite)
- [x] Transfer-Engine (Multipart, Pause/Resume, Retry)
- [x] GTK4-Basisschnittstelle
- [ ] **Transfer-Queue-UI** (Fortschrittsbalken, Warteschlange)
- [ ] **Suchfunktion** über Buckets/Objekte
- [ ] **Lesezeichen** für häufig verwendete Pfade
- [ ] **Batch-Operationen** (mehrere Dateien gleichzeitig)
- [ ] **Verschlüsselung** clientseitig (vor Upload)
- [ ] **Debian/AppImage-Paketierung** (automatisiert)
- [ ] **Flatpak**-Veröffentlichung

---

## 🤝 Beitragen

Beiträge sind willkommen! Bitte beachte:

1. **Issues** – Für Feature-Wünsche oder Bug-Reports
2. **Pull Requests** – Für Code-Änderungen
3. **ADR** – Architecture Decision Records dokumentieren wichtige Entscheidungen

---

## 📄 Lizenz

MIT © [Andreas Höfler](https://github.com/andreashoefler1985)

---

<div align="center">
  <sub>Mit ❤️ in Rust gebaut.</sub>
  <br/>
  <a href="#top">⬆ Nach oben</a>
</div>
