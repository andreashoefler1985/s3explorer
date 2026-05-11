# Tech-Stack-Entscheidung — r2

> **Projekt:** r2 — Nativer S3-kompatibler Object-Storage-Browser für Ubuntu Linux
> **Version:** 1.0
> **Status:** Final
> **Basiert auf:** SRD.md v1.0, UX_CONCEPTION.md v1.0

---

## 1. Empfohlener Stack

| Layer | Entscheidung | Alternativen | Grund |
|-------|-------------|--------------|-------|
| **Sprache** | Rust | C++, Go, Python | Speichersicherheit, Performance, starkes Typsystem, Cargo-Ökosystem |
| **UI-Framework** | GTK4 (gtk4-rs) | Qt6 (qt.rs), Iced, egui | Native Wayland-Unterstützung, libsecret-Integration, Ubuntu-Standard |
| **S3-Client** | aws-sdk-s3 (Rust) | rust-s3, MinIO Rust SDK | Vollständige SigV4, Multipart, offiziell von AWS maintained |
| **Async Runtime** | Tokio | async-std, smol | De-facto-Standard, breite Kompatibilität mit aws-sdk |
| **Credential Storage** | libsecret (Secret Service API) | keyring-crate, encrypted TOML | Native Linux-Integration, keine Klartext-Credentials |
| **Metadata Cache** | SQLite (rusqlite) | sled, redb | Robust, offline-fähig, einfach zu debuggen |
| **UI State Management** | GTK4 Property System + gtk4-rs Signals | Redux-ähnlich, Event-Bus | GTK4-nativ, keine zusätzliche Abhängigkeit |
| **Drag & Drop** | GTK4 DnD API (GdkDrag) | Eigenimplementierung | Native Desktop-Integration, Wayland-kompatibel |
| **Build System** | Cargo + cargo-deb + appimage-rust | CMake, Meson | Rust-Ökosystem, einfache .deb-Generierung |
| **TOML Parsing** | toml + serde | — | Profil-Konfiguration |
| **Logging** | tracing | log, env_logger | Strukturiertes Logging, Tokio-Integration |
| **Testing** | cargo test + rstest | — | Standard Rust-Testing |
| **Desktop Notifications** | notify-rust | — | D-Bus-Notifications, GTK4-kompatibel |
| **Clipboard** | GTK4 Clipboard API | arboard | Native Integration, kein zusätzliches Crate nötig |
| **Dateimanager-Integration** | GTK4 DnD + xdg-open (open::that) | — | Systemweites Drag & Drop, URI-Handling |

---

## 2. Begründung für Schlüsselentscheidungen

### 2.1 Rust vs. C++

**Gewählt: Rust**

| Kriterium | Rust | C++ |
|-----------|------|-----|
| Memory Safety | ✅ Garantiert durch Borrow-Checker | ❌ Manuelles Speichermanagement |
| Build-System | ✅ Cargo — de facto Standard | ⚠️ CMake + Conan/vcpkg — fragmentiert |
| S3-Client | ✅ aws-sdk-s3 — vollständig, maintained | ⚠️ aws-sdk-cpp — komplexe Build-Integration |
| UI-Bindung | ✅ gtk4-rs — mature, aktiv entwickelt | ⚠️ gtkmm — weniger aktiv |
| Async-Ökosystem | ✅ Tokio — erstklassig | ❌ Kein vergleichbares Standard-Async |
| Binärgröße | ✅ ~10-20 MB (strip) | ✅ ~5-15 MB |
| Lernkurve | ⚠️ Steil (Borrow-Checker) | ⚠️ Steil (UB, Templates) |

**C++ verworfen, weil:**
- Qt6 mit C++ wäre möglich, aber CMake + Qt MOC + S3-Client-Integration ist signifikant komplexer
- Keine vergleichbare Speichersicherheit — Use-After-Free und Buffer-Overflows sind in einer Desktop-App mit Netzwerk-I/O ein reales Risiko
- aws-sdk-cpp hat eine komplexe Build-System-Integration (CMake + vcpkg)
- Das Rust-Ökosystem (Cargo, crates.io) reduziert Dependency-Management-Aufwand drastisch

**Go verworfen, weil:**
- GTK4-Bindungen (gotk4) sind weniger mature als gtk4-rs
- Kein vergleichbares S3-SDK mit vollem SigV4-Support
- GC-basierte Speicherverwaltung kann bei vielen gleichzeitigen Transfers zu Latenzspitzen führen

**Python verworfen, weil:**
- GTK4-Bindungen (PyGObject) sind verfügbar, aber keine native Desktop-App
- Deutlich höherer RAM-Verbrauch
- Keine native Binärverteilung ohne Bundle (PyInstaller etc.)

---

### 2.2 GTK4 vs. Qt6

**Gewählt: GTK4 (gtk4-rs)**

| Kriterium | GTK4 (gtk4-rs) | Qt6 (qt.rs / C++) |
|-----------|----------------|-------------------|
| Ubuntu-Integration | ✅ Adwaita-Theme, GNOME HIG | ⚠️ Qt-Theming, nicht nativ |
| Wayland | ✅ First-Class | ✅ First-Class (Qt6) |
| libsecret | ✅ Secret Service API — nativ | ⚠️ Umweg über D-Bus |
| Rust-Bindungen | ✅ gtk4-rs — mature, aktiv | ⚠️ qt.rs — weniger mature, kleinere Community |
| Lizenz | ✅ LGPL | ⚠️ LGPL + kommerziell (Qt) |
| UI-Komplexität | ✅ Ausreichend für Desktop-App | ⚠️ Qt Quick/QML — overkill |
| Dokumentation | ⚠️ GTK4-Doku verbesserungswürdig | ✅ Hervorragend |
| Community | ✅ Große Linux-Community | ✅ Große, aber C++-lastige Community |

**Qt6 verworfen, weil:**
- qt.rs (Rust-Bindings für Qt6) sind weniger mature als gtk4-rs — das Risiko von fehlenden Features oder Bugs ist höher
- libsecret-Integration ist über D-Bus umständlicher als die native Secret-Service-API in GTK4
- Qt Quick / QML ist für eine klassische Desktop-App mit Tree-Views und Table-Views overkill
- Qt6 unter Ubuntu erfordert zusätzliche Runtime-Dependencies (qt6-* Pakete), die nicht standardmäßig installiert sind

**Iced / egui verworfen, weil:**
- Iced ist noch nicht stabil genug für eine produktive Desktop-App (API-Breaking-Changes)
- egui ist primär für ImGui-Anwendungsfälle (Tools, Debugger) konzipiert, nicht für klassische Desktop-Apps
- Beide haben keine native libsecret-Integration
- Keine systemweite Drag & Drop-Unterstützung (Dateimanager → App)

---

### 2.3 aws-sdk-s3 vs. rust-s3

**Gewählt: aws-sdk-s3 (offizielles AWS Rust SDK)**

| Kriterium | aws-sdk-s3 | rust-s3 |
|-----------|------------|---------|
| SigV4 | ✅ Vollständig | ✅ Vollständig |
| Multipart Upload | ✅ Vollständig | ⚠️ Basis |
| Multipart Download | ✅ Vollständig | ❌ Nicht unterstützt |
| CopyObject | ✅ Vollständig | ✅ Basis |
| Versioning | ✅ Vollständig | ❌ Nicht unterstützt |
| ACLs | ✅ Vollständig | ❌ Nicht unterstützt |
| Lifecycle | ✅ Vollständig | ❌ Nicht unterstützt |
| Presigned URLs | ✅ Vollständig | ✅ Basis |
| MinIO-Kompatibilität | ✅ Getestet | ✅ Getestet |
| Wasabi-Kompatibilität | ✅ Getestet | ⚠️ Nicht garantiert |
| Community | ✅ AWS-maintained, große Community | ⚠️ Kleinere Community |
| Build-Zeit | ⚠️ Lang (viele Abhängigkeiten) | ✅ Kurz |

**rust-s3 verworfen, weil:**
- Fehlende Multipart-Download-Unterstützung — kritisch für große Dateien
- Keine Versioning-API — Should-Have S-01 kann nicht umgesetzt werden
- Keine ACL-Unterstützung — Should-Have S-02 kann nicht umgesetzt werden
- Kleinere Community, weniger getestet für nicht-AWS-Endpunkte (Wasabi, Ceph)
- Keine Garantie für Kompatibilität mit allen S3-kompatiblen Backends

**MinIO Rust SDK verworfen, weil:**
- Speziell für MinIO entwickelt — keine Garantie für AWS S3 oder andere Backends
- Kleine Community, wenig Aktivität

---

### 2.4 Tokio vs. async-std / smol

**Gewählt: Tokio**

| Kriterium | Tokio | async-std | smol |
|-----------|-------|-----------|------|
| aws-sdk-s3-Kompatibilität | ✅ First-Class | ⚠️ Nicht offiziell unterstützt | ⚠️ Nicht offiziell unterstützt |
| Ökosystem | ✅ Sehr groß | ⚠️ Schrumpfend | ⚠️ Klein |
| Performance | ✅ Hervorragend | ✅ Gut | ✅ Hervorragend |
| Dokumentation | ✅ Hervorragend | ⚠️ Mittel | ⚠️ Mittel |
| Aktive Entwicklung | ✅ Ja | ⚠️ Eingestellt (2023) | ✅ Ja |
| Multi-Threaded | ✅ Work-Stealing Scheduler | ✅ Work-Stealing | ❌ Single-Threaded (standard) |

**async-std verworfen, weil:**
- Entwicklung wurde 2023 eingestellt — keine zukünftigen Updates
- aws-sdk-s3 ist primär für Tokio entwickelt und getestet
- Keine Vorteile gegenüber Tokio für diesen Anwendungsfall

**smol verworfen, weil:**
- Single-Threaded-Design ist suboptimal für parallele S3-Transfers
- Keine native Integration mit aws-sdk-s3
- Zu klein für eine produktive Desktop-App mit Netzwerk-I/O

---

### 2.5 libsecret vs. keyring-crate / encrypted TOML

**Gewählt: libsecret (Secret Service API)**

| Kriterium | libsecret | keyring-crate | encrypted TOML |
|-----------|-----------|---------------|----------------|
| Sicherheit | ✅ System Keyring | ✅ System Keyring | ⚠️ Selbst verwaltet |
| Linux-Integration | ✅ Native D-Bus | ✅ D-Bus (via libsecret) | ❌ Keine |
| GNOME Keyring | ✅ Ja | ✅ Ja | ❌ |
| KDE Wallet | ✅ Ja | ✅ Ja | ❌ |
| Offline-Fallback | ✅ Fallback auf verschlüsselte Datei | ⚠️ Abhängig von Backend | ✅ Immer verfügbar |
| Rust-Ökosystem | ✅ secret-service-rs | ✅ keyring-crate | ✅ aes-gcm + serde |
| Kontrolle | ✅ Voll | ⚠️ Abstraktionsebene | ✅ Voll |

**keyring-crate verworfen, weil:**
- keyring-crate ist eine Abstraktion über verschiedene Backends — das ist für Linux-only nicht nötig
- Direkte Nutzung von secret-service-rs gibt mehr Kontrolle über das Schema und die Fehlerbehandlung
- Weniger Abhängigkeiten (kein Plattform-Abstraktions-Layer)

**Encrypted TOML verworfen, weil:**
- Selbstverwaltete Verschlüsselung ist fehleranfällig (Key-Management, Rotation)
- Keine Integration mit dem System Keyring — Benutzer müssen sich kein zusätzliches Passwort merken
- Backup/Restore ist komplizierter als bei libsecret (das GNOME Keyring/KDE Wallet automatisch sichern)

---

### 2.6 SQLite (rusqlite) vs. sled / redb

**Gewählt: SQLite (rusqlite)**

| Kriterium | SQLite (rusqlite) | sled | redb |
|-----------|-------------------|------|------|
| Reife | ✅ 35+ Jahre | ⚠️ 5 Jahre | ⚠️ 3 Jahre |
| Offline-Fähigkeit | ✅ Embedded, keine Dependencies | ✅ Embedded | ✅ Embedded |
| Query-Möglichkeit | ✅ Volles SQL | ❌ Key-Value only | ❌ Key-Value only |
| Debugging | ✅ sqlite3 CLI, DB Browser | ⚠️ Kein Standard-Tool | ⚠️ Kein Standard-Tool |
| Transaktionen | ✅ ACID | ✅ ACID | ✅ ACID |
| Concurrent Access | ✅ WAL-Modus | ✅ Gut | ✅ Gut |
| Rust-Integration | ✅ rusqlite — mature | ✅ sled — mature | ✅ redb — aktiv |
| Backup | ✅ .sqlite-Datei kopieren | ⚠️ snapshot-Mechanismus | ⚠️ snapshot-Mechanismus |

**sled verworfen, weil:**
- Reine Key-Value-Datenbank — komplexe Queries (z.B. "alle Objekte in Prefix X mit Storage-Class Glacier") sind umständlich
- Kein Standard-Tool zum Inspizieren der Datenbank während der Entwicklung
- Weniger erprobt für Cache-Anwendungsfälle mit TTL-basierter Invalidierung

**redb verworfen, weil:**
- Noch relativ jung (3 Jahre) — Risiko von API-Änderungen
- Ebenfalls Key-Value — gleiche Einschränkungen wie sled
- Keine Vorteile gegenüber SQLite für dieses Schema

---

### 2.7 GTK4 Property System vs. Redux-ähnlich / Event-Bus

**Gewählt: GTK4 Property System + gtk4-rs Signals**

| Kriterium | GTK4 Property System | Redux-ähnlich | Event-Bus |
|-----------|---------------------|---------------|-----------|
| GTK4-Integration | ✅ Nativ | ❌ Zusätzlicher Bridge-Code | ⚠️ Möglich |
| Komplexität | ✅ Niedrig | ⚠️ Mittel | ⚠️ Mittel |
| Typensicherheit | ✅ gtk4-rs glib::Property | ✅ Ja | ⚠️ String-basiert |
| Debugging | ✅ GTK4 Inspector | ⚠️ Middleware-Logging | ⚠️ Event-Tracing |
| Zustandshistorie | ❌ Nicht vorgesehen | ✅ Time-Travel-Debugging | ❌ Nicht vorgesehen |
| Dependency | ✅ Keine | ⚠️ Zusätzliches Crate | ⚠️ Zusätzliches Crate |

**Redux-ähnlich verworfen, weil:**
- GTK4 hat ein eigenes, ausgereiftes Property/Signal-System — ein Redux-Pattern würde zusätzliche Abstraktion ohne echten Mehrwert einführen
- Time-Travel-Debugging ist für eine Desktop-App mit S3-Operationen nicht relevant
- Zusätzliche Komplexität durch Actions, Reducer, Store — für zwei Panes und eine Transfer-Queue nicht gerechtfertigt

**Event-Bus verworfen, weil:**
- String-basierte Events sind fehleranfällig (Tippfehler)
- Weniger typsicher als GTK4-Signals mit definierten Parametern
- GTK4-Signals erfüllen denselben Zweck (Pane↔Pane-Kommunikation, Drag & Drop)

---

### 2.8 Cargo + cargo-deb + appimage-rust vs. CMake / Meson

**Gewählt: Cargo + cargo-deb + appimage-rust**

| Kriterium | Cargo + cargo-deb | CMake | Meson |
|-----------|-------------------|-------|-------|
| Rust-Integration | ✅ First-Class | ⚠️ Corrosion-Bridge | ⚠️ Wrap-Dependency |
| .deb-Generierung | ✅ cargo-deb — einfach | ✅ CPack | ✅ Debhelper |
| AppImage | ✅ appimage-rust | ⚠️ Manuell | ⚠️ Manuell |
| Dependency-Management | ✅ Cargo.toml | ⚠️ vcpkg / Conan | ⚠️ Wrap |
| Build-Geschwindigkeit | ✅ Incremental Compilation | ✅ Gut | ✅ Gut |
| Konfiguration | ✅ TOML — einfach | ⚠️ CMakeLists.txt — komplex | ✅ Meson.build — einfach |

**CMake verworfen, weil:**
- Rust-Projekte mit CMake zu bauen erfordert zusätzliche Tooling (Corrosion, cargo-c)
- Kein Vorteil gegenüber Cargo für ein reines Rust-Projekt
- CMake + Cargo = zwei Build-Systeme, die synchronisiert werden müssen

**Meson verworfen, weil:**
- Wie CMake — zusätzliche Komplexität ohne Mehrwert
- Meson-Wrap-Dependencies sind weniger verbreitet als Cargo-Crates

---

## 3. Abhängigkeitsgraph (Cargo.toml)

```toml
[package]
name = "r2"
version = "0.1.0"
edition = "2021"

[dependencies]
# UI
gtk4 = { version = "0.9", features = ["v4_14"] }
libadwaita = { version = "0.7", features = ["v1_6"] }

# Async
tokio = { version = "1", features = ["full"] }

# S3
aws-sdk-s3 = "1"
aws-config = "1"
aws-credential-types = "1"

# Credential Storage
secret-service = "4"

# Cache
rusqlite = { version = "0.32", features = ["bundled"] }

# Config
toml = "0.8"
serde = { version = "1", features = ["derive"] }
serde_json = "1"

# Logging
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["json"] }

# Notifications
notify-rust = "4"

# UUID for profile IDs
uuid = { version = "1", features = ["v4"] }

# Date/Time
chrono = { version = "0.4", features = ["serde"] }

# Encryption fallback
aes-gcm = "0.10"
base64 = "0.22"

[dev-dependencies]
rstest = "0.23"
tempfile = "3"
mockall = "0.13"

[package.metadata.deb]
name = "r2"
section = "net"
priority = "optional"
maintainer = "r2 Team <dev@r2.app>"
depends = "libgtk-4-1 (>= 4.14), libsecret-1-0 (>= 0.20), libsqlite3-0 (>= 3.40)"
```

---

## 4. Rust Workspace-Struktur

```
r2/
├── Cargo.toml                  # Workspace-Definition
├── Cargo.lock
├── src/
│   ├── main.rs                 # App-Entrypoint
│   ├── app.rs                  # GTK4 Application
│   ├── config.rs               # TOML-Config-Loader
│   ├── ui/
│   │   ├── mod.rs
│   │   ├── main_window.rs      # Hauptfenster
│   │   ├── pane.rs             # S3Pane-Widget
│   │   ├── bucket_selector.rs  # Bucket-Dropdown
│   │   ├── object_list.rs      # GtkColumnView für Objekte
│   │   ├── breadcrumb.rs       # Breadcrumb-Navigation
│   │   ├── transfer_queue.rs   # Transfer-Queue-Panel
│   │   ├── profile_manager.rs  # Profil-Manager-Dialog
│   │   ├── bucket_properties.rs# Bucket-Eigenschaften-Dialog
│   │   ├── acl_editor.rs       # ACL-Editor-Dialog
│   │   ├── object_info.rs      # Objekt-Info-Panel
│   │   ├── status_bar.rs       # Statusleiste
│   │   └── drag_drop.rs        # Drag & Drop-Handler
│   ├── s3/
│   │   ├── mod.rs
│   │   ├── client.rs           # S3Client-Wrapper
│   │   ├── operations.rs       # List, Get, Put, Delete, Copy
│   │   ├── multipart.rs        # Multipart-Upload/Download
│   │   └── types.rs            # S3-Bezogene Typen
│   ├── transfer/
│   │   ├── mod.rs
│   │   ├── engine.rs           # TransferEngine (Tokio-Tasks)
│   │   ├── queue.rs            # PriorityQueue
│   │   ├── job.rs              # TransferJob-Definition
│   │   └── progress.rs         # Progress-Stream
│   ├── cache/
│   │   ├── mod.rs
│   │   ├── database.rs         # SQLite-Datenbank
│   │   ├── bucket_cache.rs     # Bucket-Cache
│   │   ├── object_cache.rs     # Objekt-Cache
│   │   └── sync.rs             # Hintergrund-Sync
│   ├── credentials/
│   │   ├── mod.rs
│   │   ├── libsecret.rs        # libsecret-Integration
│   │   └── encrypted_file.rs   # AES-256-GCM-Fallback
│   └── error.rs                # Einheitliches Error-Handling
├── tests/
│   ├── integration/
│   │   ├── s3_mock.rs          # Mock-S3-Server
│   │   ├── transfer_tests.rs   # Transfer-Integration
│   │   └── cache_tests.rs      # Cache-Integration
│   └── common/
│       └── mod.rs              # Test-Helper
├── build.rs                    # Build-Script (optional)
├── .github/
│   └── workflows/
│       └── ci.yml              # GitHub Actions CI/CD
└── assets/
    ├── icons/                  # App-Icons
    ├── r2.desktop              # .desktop-Datei
    └── r2.metainfo.xml         # AppStream-Metadaten
```

---

## 5. Nicht-Funktionale Anforderungen (Mapping)

| NFR-ID | Anforderung | Tech-Stack-Entscheidung |
|--------|-------------|------------------------|
| NFR-PERF-01 | UI bleibt reaktiv bei 10.000+ Objekten | GTK4 GtkColumnView + Lazy Loading + SQLite-Index |
| NFR-PERF-02 | Lazy Loading (100 Objekte/Page) | aws-sdk-s3 ListObjectsV2 mit MaxKeys |
| NFR-PERF-03 | App-Startzeit < 2s | Rust native Binary + SQLite-Cache |
| NFR-PERF-04 | Bucket-Listing < 1s | aws-sdk-s3 ListBuckets + Cache |
| NFR-PERF-05 | Parallele Transfers (1-16 Streams) | Tokio Work-Stealing Scheduler |
| NFR-PERF-06 | Cache-Read < 50ms | SQLite mit Index + WAL-Modus |
| NFR-SEC-01 | Credentials nie im Klartext | libsecret Secret Service API |
| NFR-SEC-02 | Secret Key nur im RAM | Entschlüsselung bei Connect |
| NFR-SEC-03 | Keine Credentials in Logs | tracing + Sensitive-Field-Filter |
| NFR-SEC-04 | TLS für alle S3-Aufrufe | aws-sdk-s3 Default (HTTPS) |
| NFR-COMPAT-01 | Wayland + X11 | GTK4 native Wayland-Unterstützung |
| NFR-COMPAT-02 | S3-Backend-Kompatibilität | aws-sdk-s3 SigV4 + konfigurierbare Endpunkte |
| NFR-COMPAT-03 | Ubuntu 22.04+ | .deb via cargo-deb |
| NFR-MAINT-01 | Modularer Code | Rust Workspace mit Sub-Crates |
| NFR-MAINT-04 | Strukturierte Logs | tracing + JSON-Format |

---

> **Dokumentversion:** 1.0
> **Erstellt:** 11. Mai 2026
> **Nächste Schritte:** ADRs erstellen, Sprint-Backlog ableiten
