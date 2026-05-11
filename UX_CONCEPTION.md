# UX/UI Conception — r2

> **Projekt:** r2 — Nativer S3-kompatibler Object-Storage-Browser für Ubuntu Linux
> **Dokumenttyp:** UX/UI Conception
> **Basiert auf:** SRD.md v1.0
> **Sprache:** Deutsch

---

## Inhaltsverzeichnis

1. [User Flows](#1-user-flows)
   - [Flow 1: App-Start und Profil-Auswahl](#flow-1-app-start-und-profil-auswahl)
   - [Flow 2: Dual-Pane-Browsing](#flow-2-dual-pane-browsing)
   - [Flow 3: S3→S3-Transfer (Drag & Drop)](#flow-3-s3s3-transfer-drag--drop)
   - [Flow 4: Lokal→S3 Upload](#flow-4-lokals3-upload)
   - [Flow 5: S3→Lokal Download](#flow-5-s3lokal-download)
   - [Flow 6: Bucket-Operationen](#flow-6-bucket-operationen)
   - [Flow 7: Transfer-Queue-Management](#flow-7-transfer-queue-management)
2. [Wireframes](#2-wireframes)
   - [Hauptfenster-Layout](#21-hauptfenster-layout)
   - [Profil-Manager-Dialog](#22-profil-manager-dialog)
   - [Transfer-Queue-Panel](#23-transfer-queue-panel)
   - [ACL-Editor-Dialog](#24-acl-editor-dialog)
   - [Bucket-Properties-Dialog](#25-bucket-properties-dialog)
   - [Objekt-Info-Panel](#26-objekt-info-panel)
3. [UI Component Specs](#3-ui-component-specs)
   - [Pane-Komponente](#31-pane-komponente)
   - [Bucket-Selector](#32-bucket-selector)
   - [Objekt-Liste (TableView)](#33-objekt-liste-tableview)
   - [Breadcrumb-Navigation](#34-breadcrumb-navigation)
   - [Transfer-Queue-Eintrag](#35-transfer-queue-eintrag)
   - [Profil-Formular](#36-profil-formular)
   - [ACL-Editor](#37-acl-editor)
   - [Statusleiste](#38-statusleiste)
4. [Interaction Patterns](#4-interaction-patterns)
   - [Drag & Drop](#41-drag--drop)
   - [Context Menus](#42-context-menus)
   - [Keyboard Shortcuts](#43-keyboard-shortcuts)
   - [Inline-Editing](#44-inline-editing)
   - [Pane-Resize](#45-pane-resize)
5. [Error / Empty / Loading States](#5-error--empty--loading-states)
   - [Profil-Manager](#51-profil-manager)
   - [Dual-Pane-Browser](#52-dual-pane-browser)
   - [Transfer-Queue](#53-transfer-queue)
   - [Bucket-Operationen](#54-bucket-operationen)
   - [ACL-Editor](#55-acl-editor)
6. [Accessibility Considerations](#6-accessibility-considerations)
   - [Tastatur-Navigation](#61-tastatur-navigation)
   - [Screenreader-Unterstützung](#62-screenreader-unterstützung)
   - [Farben und Kontraste](#63-farben-und-kontraste)
   - [Fokus-Management](#64-fokus-management)
   - [Bewegung und Animation](#65-bewegung-und-animation)

---

## 1. User Flows

### Flow 1: App-Start und Profil-Auswahl

**Auslöser:** Benutzer startet die Applikation.

**Vorbedingungen:** Keine.

**Nachbedingungen:** Ein verbundenes Profil ist aktiv, Bucket-Liste wird geladen.

**Hauptablauf:**

```
1. App startet
2. System prüft ~/.config/r2/profiles.toml auf existierende Profile
   │
   ├── Keine Profile gefunden →
   │   3a. Profil-Manager-Dialog öffnet sich automatisch (leer)
   │   4a. Benutzer klickt "Neu"
   │   5a. Profil-Formular öffnet sich
   │   6a. Benutzer füllt: Name, Endpoint URL, Access Key, Secret Key, Region
   │   7a. Benutzer klickt "Test Connection"
   │       ├── Erfolg → grüner Haken, "Verbindung erfolgreich"
   │       └── Fehler → rote Meldung mit Fehlerdetails
   │   8a. Benutzer klickt "Speichern"
   │   9a. Profil wird gespeichert (Config + libsecret)
   │  10a. Dialog schließt, Hauptfenster öffnet sich mit verbundenem Profil
   │
   └── Profile gefunden →
       3b. Hauptfenster öffnet sich mit letztem aktivem Profil
       4b. Automatischer Verbindungsaufbau zum letzten Profil
           ├── Erfolg → Bucket-Liste wird geladen, Status "Verbunden"
           └── Fehler → Status "Getrennt", Fehlerhinweis in Statusleiste
       5b. Benutzer kann über Toolbar-Dropdown Profil wechseln
```

**Alternativabläufe:**

| Schritt | Abweichung | Reaktion |
|---------|-----------|----------|
| 6a | Benutzer schließt Dialog ohne Speichern | App bleibt geöffnet, kein Profil aktiv, Status "Kein Profil" |
| 7a | Test Connection schlägt fehl (Timeout) | Meldung "Endpunkt nicht erreichbar — Timeout nach 30s" |
| 7a | Test Connection schlägt fehl (Auth) | Meldung "Zugangsdaten ungültig — HTTP 403" |
| 4b | Letztes Profil existiert nicht mehr (Config gelöscht) | Profil wird übersprungen, Status "Kein Profil" |

**State-Diagramm:**

```mermaid
stateDiagram-v2
    [*] --> CheckingProfiles: App start
    CheckingProfiles --> NoProfiles: profiles.toml leer/fehlt
    CheckingProfiles --> HasProfiles: Profile gefunden

    NoProfiles --> ProfileManagerOpen: Dialog öffnen
    ProfileManagerOpen --> CreatingProfile: "Neu" klicken
    CreatingProfile --> TestingConnection: "Test Connection"
    TestingConnection --> CreatingProfile: Fehler
    TestingConnection --> SavingProfile: Erfolg
    SavingProfile --> MainWindowConnected: Gespeichert

    HasProfiles --> MainWindowConnecting: Letztes Profil laden
    MainWindowConnecting --> MainWindowConnected: Verbunden
    MainWindowConnecting --> MainWindowDisconnected: Fehler

    MainWindowDisconnected --> ProfileManagerOpen: Profil wechseln
    MainWindowConnected --> ProfileManagerOpen: Profil wechseln
    MainWindowConnected --> [*]: App schließen
    MainWindowDisconnected --> [*]: App schließen
```

---

### Flow 2: Dual-Pane-Browsing

**Auslöser:** Hauptfenster ist geöffnet, mindestens ein Profil ist verbunden.

**Vorbedingungen:** Mindestens ein S3-Profil ist aktiv und verbunden.

**Nachbedingungen:** Beide Panes zeigen eine Objekt-Liste an.

**Hauptablauf:**

```
1. Hauptfenster zeigt zwei Panes nebeneinander
2. Linkes Pane (Pane A):
   a. Bucket-Selector zeigt verfügbare Buckets des aktiven Profils
   b. Benutzer wählt Bucket "my-bucket" aus Dropdown
   c. Objekt-Liste lädt (Lazy Loading, 100 Objekte pro Page)
   d. Benutzer navigiert in Prefix "images/" per Doppelklick
   e. Breadcrumb aktualisiert: Bucket > images >
   f. Objekt-Liste zeigt Inhalt von "images/"
3. Rechtes Pane (Pane B):
   a. Bucket-Selector zeigt Buckets (anderes Profil oder selbes Profil)
   b. Benutzer wählt Bucket "other-bucket"
   c. Navigation analog zu Pane A
4. Beide Panes sind unabhängig bedienbar
5. Benutzer kann zwischen Panes wechseln (Klick oder Tab)
```

**Pane-Interaktionen:**

| Aktion | Ergebnis |
|--------|----------|
| Doppelklick auf Ordner/Prefix | Navigation in den Ordner |
| Doppelklick auf Datei | (Could-Have) Vorschau / "Im Browser öffnen" |
| Klick auf Breadcrumb-Ebene | Navigation zu dieser Ebene |
| Klick auf "↑" (Parent) | Navigation eine Ebene höher |
| Rechtsklick auf Objekt | Context Menu (Download, Delete, Rename, Copy, Properties) |
| Mehrfachauswahl (Strg+Klick / Shift+Klick) | Batch-Operationen möglich |
| Spaltenkopf klicken | Sortierung der Objekt-Liste |
| Scrollen am Ende | Automatisches Nachladen (Lazy Loading) |

**Profil-Wechsel pro Pane:**

```
1. Benutzer klickt auf Profil-Dropdown im Pane-Header
2. Dropdown zeigt alle gespeicherten Profile + "Lokal" (für lokale Ordner)
3. Auswahl wechselt das Profil für DIESES Pane
4. Bucket-Liste wird neu geladen
5. Das andere Pane bleibt unverändert
```

---

### Flow 3: S3→S3-Transfer (Drag & Drop)

**Auslöser:** Benutzer zieht ein oder mehrere Objekte von Pane A (Quelle) auf Pane B (Ziel).

**Vorbedingungen:** Beide Panes sind mit S3-Endpunkten verbunden. Ziel-Pfad ist sichtbar.

**Nachbedingungen:** Transfer ist in der Queue, Fortschritt wird angezeigt.

**Hauptablauf:**

```
1. Benutzer selektiert Objekt(e) in Pane A (Quelle)
2. Benutzer beginnt Drag (mousedown + move)
3. Drag-Overlay zeigt: "[n] Dateien" + Quell-Pfad
4. Benutzer zieht über Pane B (Ziel)
   a. Pane B zeigt visuelles Drop-Target-Highlight (grüner Rahmen)
   b. Cursor wechselt zu "Copy"-Symbol (+)
5. Benutzer droppt auf Pane B
6. System prüft:
   a. Sind Quelle und Ziel identisch? → Kopie innerhalb des Buckets
   b. Sind Profile identisch? → S3→S3-Kopie (CopyObject)
   c. Sind Profile unterschiedlich? → S3→S3-Download+Upload
7. Transfer-Queue öffnet sich (Slide-in von unten)
8. Für jede Datei wird ein Queue-Eintrag erstellt
9. Parallele Transfers starten (Default: 4 gleichzeitig)
10. Fortschrittsanzeige pro Datei: ████████░░ 80% | 5MB/s | ETA: 10s
11. Bei Abschluss: Eintrag wechselt zu "Completed" (grün)
12. Benutzer kann Pane B refreshen, um neue Objekte zu sehen
```

**Drag & Drop Visual Feedback:**

| Zustand | Visuelles Feedback |
|---------|-------------------|
| Drag startet | Verkleinertes Objekt-Icon folgt Mauszeiger |
| Über gültigem Ziel (Pane B) | Grüner Rahmen um Pane B, "+" Cursor |
| Über ungültigem Ziel (gleiches Pane, kein Bucket) | Rotes "✕" Cursor, kein Highlight |
| Drop erfolgt | Kurze Animation, Queue öffnet sich |
| Drop auf Ordner/Prefix | Objekt wird in diesen Prefix transferiert |

**Fehlerbehandlung:**

```
1. Transfer schlägt fehl (Netzwerk, Auth, Quota)
2. Automatischer Retry mit exponentiellem Backoff (3 Versuche)
3. Nach 3 Fehlversuchen: Status → "Failed" (rot)
4. Fehlermeldung wird angezeigt (z.B. "Zugriff verweigert — HTTP 403")
5. Benutzer kann "Retry" klicken für manuellen Wiederholungsversuch
6. Benutzer kann "Resume All Failed" für alle fehlgeschlagenen Transfers
```

---

### Flow 4: Lokal→S3 Upload

**Auslöser:** Benutzer möchte Dateien vom lokalen Dateisystem in einen S3-Bucket hochladen.

**Vorbedingungen:** Ein Pane ist mit einem S3-Endpunkt verbunden und zeigt einen Ziel-Pfad.

**Hauptablauf (Drag & Drop aus Dateimanager):**

```
1. Benutzer öffnet Nautilus/Dolphin (Dateimanager)
2. Benutzer selektiert Datei(en) und/oder Ordner
3. Benutzer zieht Dateien in das r2-Hauptfenster
   a. Pane zeigt Drop-Target-Highlight (grüner Rahmen)
   b. Cursor zeigt "Copy"-Symbol
4. Benutzer droppt auf Ziel-Pane (in gewünschten Prefix)
5. System erkennt: Quelle = lokal, Ziel = S3
6. Für jede Datei wird ein Upload-Job erstellt
7. Für Ordner: Rekursives Hochladen mit Struktur-Erhalt
8. Transfer-Queue öffnet sich
9. Multipart-Upload für Dateien > 100 MB (automatisch)
10. Fortschritt wird angezeigt
```

**Hauptablauf (Upload-Button):**

```
1. Benutzer klickt "Upload"-Button in Pane-Toolbar
2. Native File-Auswahldialog öffnet sich (GTK4 FileChooserNative)
3. Benutzer wählt Datei(en) aus (Mehrfachauswahl möglich)
4. Benutzer klickt "Öffnen"
5. Upload-Jobs werden erstellt
6. Transfer-Queue öffnet sich
7. Upload startet
```

**Multipart-Upload-Logik:**

| Dateigröße | Upload-Strategie |
|-----------|-----------------|
| < 100 MB | Single PUT-Request |
| 100 MB – 5 GB | Multipart, 50 MB Parts (konfigurierbar) |
| > 5 GB | Multipart, 50 MB Parts, max. 10.000 Parts |

**Ordner-Upload:**

```
1. Benutzer droppt Ordner "projekt/" in Pane
2. System listet alle Dateien rekursiv
3. Relative Pfade werden erhalten:
   - lokal: /home/user/projekt/src/main.rs
   - S3:    /bucket/projekt/src/main.rs
4. Fortschritt: "Datei 47/156: src/main.rs"
5. Bei Abbruch: Bereits hochgeladene Dateien bleiben erhalten
```

---

### Flow 5: S3→Lokal Download

**Auslöser:** Benutzer möchte Objekte aus S3 auf das lokale Dateisystem herunterladen.

**Vorbedingungen:** Ein Pane zeigt S3-Objekte an.

**Hauptablauf (Rechtsklick → Download):**

```
1. Benutzer rechtsklickt auf Objekt(e) in Pane
2. Context Menu → "Herunterladen..."
3. Native Folder-Auswahldialog öffnet sich (GTK4)
4. Benutzer wählt Zielordner
5. Download-Jobs werden erstellt
6. Transfer-Queue öffnet sich
7. Fortschritt wird angezeigt
8. Bei Abschluss: Desktop-Notification (optional)
```

**Hauptablauf (Drag & Drop auf Dateimanager):**

```
1. Benutzer selektiert Objekt(e) in Pane
2. Benutzer zieht Objekte aus r2-Fenster
3. r2-Fenster bleibt im Vordergrund (GTK4 DnD)
4. Benutzer droppt auf Nautilus-Fenster oder Desktop
5. System fragt Zielordner (falls nicht eindeutig)
6. Download startet
```

**Ordner-Download:**

```
1. Benutzer rechtsklickt auf Prefix/Ordner
2. Context Menu → "Ordner herunterladen..."
3. Zielordner wählen
4. System listet alle Objekte im Prefix rekursiv
5. Relative Pfade werden lokal nachgebildet:
   - S3:   /bucket/images/2024/photo.jpg
   - Lokal: /home/user/downloads/images/2024/photo.jpg
6. Fortschritt: "Datei 12/89: images/2024/photo.jpg"
```

---

### Flow 6: Bucket-Operationen

**Auslöser:** Benutzer möchte Buckets verwalten (erstellen, löschen, konfigurieren).

**Vorbedingungen:** Ein Profil ist verbunden.

**Bucket erstellen:**

```
1. Rechtsklick in Bucket-Liste (linke Spalte des Panes)
2. Context Menu → "Bucket erstellen..."
3. Dialog öffnet sich:
   a. Feld: Bucket-Name (mit Validierung: nur Kleinbuchstaben, Zahlen, Punkte, Bindestriche)
   b. Dropdown: Region (vorausgewählt: Profil-Region)
   c. Checkbox: "Versioning aktivieren"
4. Benutzer klickt "Erstellen"
5. API-Call: CreateBucket
   ├── Erfolg → Bucket erscheint in Liste, automatisch selektiert
   └── Fehler → Fehlermeldung (z.B. "Bucket-Name bereits vergeben")
```

**Bucket löschen:**

```
1. Rechtsklick auf Bucket in Bucket-Liste
2. Context Menu → "Bucket löschen..."
3. Bestätigungsdialog:
   a. Warnung: "Bucket 'my-bucket' und ALLE darin enthaltenen Objekte löschen?"
   b. Checkbox: "Ich bin mir bewusst, dass dieser Vorgang unwiderruflich ist"
   c. Buttons: "Abbrechen" | "Löschen" (deaktiviert bis Checkbox aktiv)
4. Benutzer bestätigt
5. API-Call: DeleteBucket
   ├── Erfolg → Bucket verschwindet aus Liste
   └── Fehler → "Bucket nicht leer" (muss erst geleert werden)
```

**Versioning togglen:**

```
1. Rechtsklick auf Bucket → "Eigenschaften"
2. Tab "Versioning"
3. Aktueller Status: "Aktiviert" / "Deaktiviert" / "Suspendiert"
4. Button: "Versioning aktivieren" / "Versioning deaktivieren"
5. Bestätigungsdialog bei Deaktivierung
```

---

### Flow 7: Transfer-Queue-Management

**Auslöser:** Transfer-Queue-Panel wird geöffnet (automatisch bei Transfer oder manuell).

**Vorbedingungen:** Mindestens ein Transfer wurde initiiert.

**Hauptablauf:**

```
1. Transfer-Queue-Panel öffnet sich (Slide-in von unten, ca. 200px Höhe)
2. Panel zeigt drei Tabs: "Aktiv" | "Abgeschlossen" | "Fehlgeschlagen"
3. Tab "Aktiv":
   a. Liste aller laufenden und pausierten Transfers
   b. Pro Eintrag:
      - Icon: Datei-Typ / Ordner
      - Dateiname
      - Quelle → Ziel (verkürzt: "bucket-a:/pfad/ → bucket-b:/pfad/")
      - Fortschrittsbalken ████████░░ 80%
      - Geschwindigkeit: "5.2 MB/s"
      - ETA: "10 Sekunden"
      - Buttons: [⏸ Pause] [✕ Abbrechen]
   c. Header: "3 aktiv, 2 pausiert"
4. Tab "Abgeschlossen":
   a. Liste aller erfolgreich abgeschlossenen Transfers
   b. Pro Eintrag: Dateiname, Größe, Dauer, Durchschnitts-Speed
   c. Button: "Alle abschließen löschen"
5. Tab "Fehlgeschlagen":
   a. Liste aller fehlgeschlagenen Transfers
   b. Pro Eintrag: Dateiname, Fehlermeldung, Zeitpunkt
   c. Buttons: [↻ Wiederholen] [✕ Entfernen]
   d. Button: "Alle fehlgeschlagenen wiederholen"
6. Benutzer kann Panel minimieren (kleiner Pfeil-Button)
7. Panel bleibt sichtbar, solange aktive Transfers laufen
```

**Transfer pausieren/fortsetzen:**

```
1. Benutzer klickt [⏸] auf aktivem Transfer
2. Transfer wird pausiert (aktueller Part wird abgeschlossen)
3. Status wechselt zu "Pausiert" (gelb)
4. Button wechselt zu [▶ Fortsetzen]
5. Benutzer klickt [▶]
6. Transfer wird fortgesetzt (nächster Part startet)
```

**Transfer abbrechen:**

```
1. Benutzer klickt [✕] auf aktivem/pausiertem Transfer
2. Bestätigungsdialog: "Transfer 'datei.zip' wirklich abbrechen?"
3. Bei Bestätigung:
   a. Aktueller Part wird abgebrochen
   b. Bei Multipart: AbortMultipartUpload wird gesendet
   c. Bereits hochgeladene Parts werden verworfen
   d. Eintrag wird aus Queue entfernt
```

---

## 2. Wireframes

### 2.1 Hauptfenster-Layout

```
┌──────────────────────────────────────────────────────────────────────────────┐
│ [☰ r2]  r2 — Object Storage Browser                           [−] [□] [×]  │
├──────────────────────────────────────────────────────────────────────────────┤
│ [≡ Datei] [≡ Bearbeiten] [≡ Ansicht] [≡ Transfer] [≡ Hilfe]                 │
├──────────────────────────────────────────────────────────────────────────────┤
│ [Profil: production ▼] [Bucket: my-bucket ▼] [Pfad: /images/] [↻] [↑] [≡]  │
├──────────────────────────────────────┬───────────────────────────────────────┤
│  Pane A (Quelle)                      │  Pane B (Ziel)                       │
│ ┌────────────────────────────────┐   │ ┌─────────────────────────────────┐  │
│ │ Profil: production        [▼] │   │ │ Profil: staging            [▼] │  │
│ │ Bucket: my-bucket    [▼] [↻] │   │ │ Bucket: other-bucket  [▼] [↻] │  │
│ │ Pfad: /images/           [↑] │   │ │ Pfad: /backups/           [↑] │  │
│ ├────────────────────────────────┤   │ ├─────────────────────────────────┤  │
│ │ Buckets          │ Objekte     │   │ │ Buckets          │ Objekte      │  │
│ │ ─────────────── │ ─────────── │   │ │ ─────────────── │ ──────────── │  │
│ │ 📦 my-bucket    │ 📁 2024/    │   │ │ 📦 other-bucket │ 📁 daily/     │  │
│ │ 📦 logs         │ 📁 2025/    │   │ │ 📦 archive      │ 📄 backup-01  │  │
│ │ 📦 backups      │ 📄 photo    │   │ │ 📦 temp         │ 📄 backup-02  │  │
│ │                 │ 📄 doc.pdf  │   │ │                 │ 📄 backup-03  │  │
│ │                 │ ...         │   │ │                 │ ...           │  │
│ │                 │             │   │ │                 │               │  │
│ ├────────────────────────────────┤   │ ├─────────────────────────────────┤  │
│ │ 42 Objekte | 156 MB           │   │ │ 12 Objekte | 6.5 GB             │  │
│ └────────────────────────────────┘   │ └─────────────────────────────────┘  │
├──────────────────────────────────────┴───────────────────────────────────────┤
│ [🔲 Transfer-Queue ▾]  3 aktiv · 2 pausiert · 15 abgeschlossen              │
├──────────────────────────────────────────────────────────────────────────────┤
│ [📄] file1.zip   production:my-bucket → staging:other-bucket                │
│      ████████░░ 80% | 5.2 MB/s | ETA: 10s                    [⏸] [✕]      │
│ [📄] file2.tar   production:my-bucket → staging:other-bucket                │
│      ████░░░░░░ 40% | 2.1 MB/s | ETA: 2min 34s              [⏸] [✕]      │
│ [📁] projekt/    /home/user/projekt → production:my-bucket                  │
│      ██░░░░░░░░ 18% | 8.7 MB/s | ETA: 5min 12s              [⏸] [✕]      │
├──────────────────────────────────────────────────────────────────────────────┤
│ [🟢 Verbunden: production]  [🟡 Verbunden: staging]  [📦 5 Buckets]  [⚡ 0] │
└──────────────────────────────────────────────────────────────────────────────┘
```

### 2.2 Profil-Manager-Dialog

```
┌────────────────────────────────────────────────────────────────┐
│  Profil-Manager                                      [×]      │
├────────────────────────────────────────────────────────────────┤
│ ┌────────────────────────────────────────────────────────────┐ │
│ │ Profile                                                    │ │
│ │ ────────────────────────────────────────────────────────── │ │
│ │ ◉ production    ● s3.eu-central-1.amazonaws.com  [🟢]     │ │
│ │ ○ staging       ● s3.eu-west-1.amazonaws.com     [🔴]     │ │
│ │ ○ minio-local   ● http://localhost:9000           [🟢]     │ │
│ │ ○ wasabi        ● s3.wasabisys.com               [⚪]     │ │
│ │ ────────────────────────────────────────────────────────── │ │
│ │ [+ Neu]  [✏ Bearbeiten]  [🗑 Löschen]  [📋 Duplizieren]   │ │
│ └────────────────────────────────────────────────────────────┘ │
│                                                               │
│ ┌────────────────────────────────────────────────────────────┐ │
│ │ Profil-Details                                             │ │
│ │ ────────────────────────────────────────────────────────── │ │
│ │ Name:             [production                    ]        │ │
│ │ Endpoint-URL:     [https://s3.eu-central-1.amazonaws.com] │ │
│ │ Access Key:       [AKIAIOSFODNN7EXAMPLE          ]        │ │
│ │ Secret Key:       [••••••••••••••••             ]        │ │
│ │ Region:           [eu-central-1          ▼]               │ │
│ │ Default Bucket:   [my-app-data               ]            │ │
│ │ ☐ Path-Style-URLs (z.B. für MinIO)                       │ │
│ │                                                           │ │
│ │ [🔍 Test Connection]  [💾 Speichern]  [Abbrechen]        │ │
│ └────────────────────────────────────────────────────────────┘ │
├────────────────────────────────────────────────────────────────┤
│  💡 Secrets werden sicher im System Keyring gespeichert.      │
└────────────────────────────────────────────────────────────────┘
```

### 2.3 Transfer-Queue-Panel

```
┌──────────────────────────────────────────────────────────────────────────────┐
│  Transfer-Queue  [−]                                                        │
├──────────────────────────────────────────────────────────────────────────────┤
│  [📋 Aktiv (5)]  [✅ Abgeschlossen (15)]  [❌ Fehlgeschlagen (2)]           │
├──────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  ┌────────────────────────────────────────────────────────────────────────┐ │
│  │ [📄] report.pdf                                                        │ │
│  │      production:my-bucket/images/ → staging:other-bucket/backups/      │ │
│  │      ████████████████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 62%            │ │
│  │      4.2 MB/s  |  ETA: 1min 23s  |  26.4 MB / 42.8 MB   [⏸] [✕]     │ │
│  ├────────────────────────────────────────────────────────────────────────┤ │
│  │ [📄] backup.tar.gz                                                      │ │
│  │      production:my-bucket/ → staging:other-bucket/                      │ │
│  │      ██████████████████████████████████████████████████████ 100%        │ │
│  │      8.1 MB/s  |  Abgeschlossen: 12:34:56                [✕]          │ │
│  ├────────────────────────────────────────────────────────────────────────┤ │
│  │ [📁] projekt/ (47 Dateien)                                              │ │
│  │      /home/user/projekt/ → production:my-bucket/projekt/                │ │
│  │      ██████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 22%          │ │
│  │      6.7 MB/s  |  ETA: 4min 12s  |  Datei 12/47            [⏸] [✕]    │ │
│  └────────────────────────────────────────────────────────────────────────┘ │
│                                                                              │
│  [▶ Alle pausierten fortsetzen]  [↻ Alle fehlgeschlagenen wiederholen]      │
│  [✕ Alle abgeschlossenen löschen]                                           │
└──────────────────────────────────────────────────────────────────────────────┘
```

### 2.4 ACL-Editor-Dialog

```
┌────────────────────────────────────────────────────────────────┐
│  ACL-Editor: my-bucket                              [×]      │
├────────────────────────────────────────────────────────────────┤
│  Bucket: my-bucket                                             │
│  Typ: Bucket-ACL                                               │
├────────────────────────────────────────────────────────────────┤
│  Besitzer: AIDACKCEVSQ6C2EXAMPLE                               │
│                                                               │
│  Berechtigungen:                                               │
│  ┌──────────────────────────────────────────────────────────┐ │
│  │ Grantee                    │ Berechtigung      │ [Aktion] │ │
│  │ ──────────────────────────────────────────────────────── │ │
│  │ 👤 Besitzer (AIDACK...)   │ Full Control      │ —        │ │
│  │ 👥 Alle (AllUsers)        │ Read              │ [✕]     │ │
│  │ 👥 Authentifizierte       │ Read              │ [✕]     │ │
│  │    (AuthenticatedUsers)   │                   │          │ │
│  │ 📧 user@example.com       │ Read, Write       │ [✕]     │ │
│  └──────────────────────────────────────────────────────────┘ │
│                                                               │
│  [+ Grantee hinzufügen]                                       │
│                                                               │
│  Neuer Grant:                                                  │
│  ┌──────────────────────────────────────────────────────────┐ │
│  │ Grantee-Typ: [Canonical User ▼]                          │ │
│  │ ID:        [                                    ]        │ │
│  │ Berechtigungen: ☑ Read  ☐ Write  ☐ ReadACP  ☐ WriteACP  │ │
│  │                ☐ FullControl                             │ │
│  │ [➕ Hinzufügen]                                           │ │
│  └──────────────────────────────────────────────────────────┘ │
│                                                               │
│  Canned ACL: [private          ▼]  [Anwenden]                 │
│                                                               │
│  [💾 Speichern]  [Abbrechen]                                  │
└────────────────────────────────────────────────────────────────┘
```

### 2.5 Bucket-Properties-Dialog

```
┌────────────────────────────────────────────────────────────────┐
│  Bucket-Eigenschaften: my-bucket                     [×]      │
├────────────────────────────────────────────────────────────────┤
│                                                               │
│  ┌──────────────────────────────────────────────────────────┐ │
│  │ 📊 Allgemein                                              │ │
│  │ ──────────────────────────────────────────────────────── │ │
│  │ Name:           my-bucket                                 │ │
│  │ Region:         eu-central-1                              │ │
│  │ Erstellt:       2024-03-15 10:30:00 UTC                   │ │
│  │ Objekte:        1.247                                     │ │
│  │ Gesamtgröße:    156.3 GB                                  │ │
│  └──────────────────────────────────────────────────────────┘ │
│                                                               │
│  ┌──────────────────────────────────────────────────────────┐ │
│  │ 🔄 Versioning                                             │ │
│  │ ──────────────────────────────────────────────────────── │ │
│  │ Status: [🟢 Aktiviert]                                    │ │
│  │ [Versioning deaktivieren]                                 │ │
│  └──────────────────────────────────────────────────────────┘ │
│                                                               │
│  ┌──────────────────────────────────────────────────────────┐ │
│  │ 🔒 Berechtigungen (ACL)                                   │ │
│  │ ──────────────────────────────────────────────────────── │ │
│  │ [ACL bearbeiten...]                                       │ │
│  └──────────────────────────────────────────────────────────┘ │
│                                                               │
│  [Schließen]                                                  │
└────────────────────────────────────────────────────────────────┘
```

### 2.6 Objekt-Info-Panel

```
┌────────────────────────────────────────────────────────────────┐
│  📄 Objekt-Informationen                            [×]      │
├────────────────────────────────────────────────────────────────┤
│                                                               │
│  Name:          photo-2024-01.jpg                             │
│  Größe:         5.2 MB (5.456.789 Bytes)                      │
│  Typ:           image/jpeg                                    │
│  ETag:          "abc123def456..."                              │
│  Storage Class: STANDARD                                      │
│  Zuletzt geändert: 2024-06-15 14:23:11 UTC                    │
│  Version-ID:    null (nicht versioniert)                      │
│                                                               │
│  ─── Pfad ───                                                 │
│  Bucket:        my-bucket                                     │
│  Key:           images/2024/photo-2024-01.jpg                 │
│  URL:           s3://my-bucket/images/2024/photo-2024-01.jpg  │
│                                                               │
│  ─── Aktionen ───                                             │
│  [📋 URL kopieren]  [
│  [Schließen]
└────────────────────────────────────────────────────────────────┘

---

## 3. UI Component Specs

### 3.1 Pane-Komponente

**Typ:** Custom GTK4 Widget (GtkBox mit Sub-Widgets)

**Struktur (vertikal):**

```
┌─────────────────────────────────────┐
│ Pane-Header                         │  ← GtkBox (horizontal)
│ [Profil-Dropdown] [Bucket-Dropdown] │
│ [Pfad-Eingabe] [↻] [↑] [≡ Aktionen]│
├─────────────────────────────────────┤
│ Split-View (horizontal)             │  ← GtkPaned
│ ┌──────────┬──────────────────────┐ │
│ │ Bucket-  │ Objekt-Liste         │ │
│ │ Tree     │ (GtkColumnView)      │ │
│ │ (GtkTree │                      │ │
│ │ View)    │                      │ │
│ └──────────┴──────────────────────┘ │
├─────────────────────────────────────┤
│ Pane-Footer                         │  ← GtkBox (horizontal)
│ "42 Objekte | 156 MB"              │
└─────────────────────────────────────┘
```

**Properties:**

| Property | Typ | Default | Beschreibung |
|----------|-----|---------|--------------|
| `profile_id` | Option\<String> | None | Aktuell verbundenes Profil |
| `bucket_name` | Option\<String> | None | Aktuell ausgewählter Bucket |
| `current_prefix` | String | "" | Aktueller Pfad/Prefix |
| `is_local` | Bool | false | True wenn lokaler Ordner geladen |
| `local_path` | Option\<PathBuf> | None | Pfad zum lokalen Ordner |

**Signals:**

| Signal | Parameter | Beschreibung |
|--------|-----------|--------------|
| `profile-changed` | profile_id | Profil wurde gewechselt |
| `bucket-changed` | bucket_name | Bucket wurde gewechselt |
| `prefix-changed` | prefix | Navigation zu anderem Prefix |
| `objects-selected` | Vec\<ObjectInfo> | Objekt(e) selektiert |
| `drop-files` | Vec\<String>, target_prefix | Dateien wurden gedroppt |
| `drop-objects` | Vec\<ObjectInfo>, target_pane | Objekte wurden auf anderes Pane gezogen |
| `upload-requested` | — | Upload-Button geklickt |
| `refresh-requested` | — | Refresh-Button geklickt |

---

### 3.2 Bucket-Selector

**Typ:** GtkDropDown (kombiniert mit GtkTreeListModel)

**Verhalten:**

| Zustand | Anzeige |
|---------|---------|
| Nicht verbunden | "— Kein Profil —" (deaktiviert) |
| Verbunden, Buckets laden | "Buckets werden geladen..." (Spinner) |
| Verbunden, Buckets geladen | Dropdown-Liste aller Buckets |
| Verbunden, keine Buckets | "— Keine Buckets —" (deaktiviert) |
| Verbunden, Fehler | "Fehler beim Laden" (rot, deaktiviert) |

**Context Menu (Rechtsklick auf Bucket-Liste):**

| Menü-Eintrag | Aktion |
|-------------|--------|
| Bucket öffnen | Bucket im Pane laden |
| Bucket erstellen... | CreateBucket-Dialog öffnen |
| Bucket löschen... | DeleteBucket-Bestätigung |
| Eigenschaften | Bucket-Properties-Dialog |
| ACL bearbeiten | ACL-Editor-Dialog |
| In anderem Pane öffnen | Bucket im anderen Pane laden |

---

### 3.3 Objekt-Liste (TableView)

**Typ:** GtkColumnView mit GtkNoSelection oder GtkMultiSelection

**Spalten:**

| Spalte | Breite | Sortierbar | Format |
|--------|--------|-----------|--------|
| Name | Flex (min 200px) | Ja | Icon + Text (Ordner fett, Dateien normal) |
| Größe | 100px | Ja | Human-readable (KB, MB, GB, TB) |
| Typ | 120px | Ja | MIME-Type / "Ordner" |
| Zuletzt geändert | 160px | Ja | Datum + Uhrzeit (relativ: "vor 2h" / absolut) |
| Storage Class | 100px | Ja | STANDARD, GLACIER, etc. |

**Zeilen-Höhe:** 32px (Standard), 40px (bei Touch)

**Interaktionen:**

| Aktion | Ergebnis |
|--------|----------|
| Einfachklick | Selektion (Einzel-/Mehrfachauswahl) |
| Doppelklick auf Ordner | Navigation in Ordner |
| Doppelklick auf Datei | (Could-Have) Vorschau / Download |
| Rechtsklick | Context Menu |
| Strg+A | Alle selektieren |
| Entf | Löschen (mit Bestätigung) |
| F2 | Umbenennen (Inline-Edit) |

**Lazy Loading:**

```
1. Initial: Erste 100 Objekte laden
2. Beim Scrollen ans Ende: Nächste 100 Objekte nachladen
3. Während des Ladens: Spinner in letzter Zeile
4. Alle geladen: "— Ende der Liste —" in letzter Zeile
```

---

### 3.4 Breadcrumb-Navigation

**Typ:** Custom GTK4 Widget (GtkBox mit klickbaren GtkButtons/Labels)

**Darstellung:**

```
🏠 Bucket > images > 2024 > >
```

**Verhalten:**

| Element | Klick |
|---------|-------|
| 🏠 (Root) | Zurück zur Bucket-Liste |
| Bucket-Name | Zurück zur Bucket-Wurzel |
| Prefix-Ebene | Navigation zu dieser Ebene |
| > (Trennzeichen) | Nicht klickbar (nur visuell) |
| Letztes Element | Aktuelle Ebene (hervorgehoben, nicht klickbar) |

**Pfad-Eingabe:**

- Klick auf Pfad-Bereich wechselt zu editierbarem GtkEntry
- Enter: Navigation zum eingegebenen Prefix
- Escape: Abbruch, Breadcrumb bleibt

---

### 3.5 Transfer-Queue-Eintrag

**Typ:** Custom GTK4 Widget (GtkListBoxRow)

**Layout:**

```
┌──────────────────────────────────────────────────────────────────────┐
│ [📄] dateiname.zip                                                   │
│      quelle:pfad → ziel:pfad                                         │
│      ████████████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 62%          │
│      4.2 MB/s  |  ETA: 1min 23s  |  26.4 MB / 42.8 MB   [⏸] [✕]   │
└──────────────────────────────────────────────────────────────────────┘
```

**Status-Farben:**

| Status | Farbe | Icon |
|--------|-------|------|
| Active | Blau (#3584e4) | ▶ |
| Paused | Gelb (#f5c211) | ⏸ |
| Completed | Grün (#33d17a) | ✅ |
| Failed | Rot (#e01b24) | ❌ |
| Pending | Grau (#9a9996) | ⏳ |

**Fortschrittsbalken:**

- GtkLevelBar mit kontinuierlichem Modus
- Farbe: Blau (aktiv), Grün (completed), Rot (failed)
- Tooltip: "26.4 MB von 42.8 MB (62%)"

---

### 3.6 Profil-Formular

**Typ:** GtkDialog mit GtkGrid-Layout

**Felder:**

| Feld | Widget | Validierung |
|------|--------|-------------|
| Name | GtkEntry | Nicht leer, max 64 Zeichen |
| Endpoint-URL | GtkEntry | Valide URL (http/https), nicht leer |
| Access Key | GtkEntry | Nicht leer, 16-128 Zeichen |
| Secret Key | GtkPasswordEntry | Nicht leer, min 8 Zeichen |
| Region | GtkDropDown | Vordefinierte Liste + Custom-Eingabe |
| Default Bucket | GtkEntry | Optional, nur gültige Bucket-Namen |
| Path-Style | GtkSwitch | Boolean |

**Test Connection Button:**

```
1. Button wird geklickt
2. Button: "Teste Verbindung..." (deaktiviert, Spinner)
3. API-Call: ListBuckets (Timeout: 30s)
   ├── Erfolg (HTTP 200) → Button grün: "✅ Verbindung erfolgreich"
   └── Fehler → Button rot: "❌ Fehler: [Meldung]"
4. Nach 5s: Button zurück zu Normalzustand
```

---

### 3.7 ACL-Editor

**Typ:** GtkDialog mit GtkGrid-Layout

**Komponenten:**

| Komponente | Typ | Beschreibung |
|-----------|-----|-------------|
| Grantee-Liste | GtkColumnView | Tabelle: Grantee, Berechtigung, Aktion |
| Grantee-Typ | GtkDropDown | CanonicalUser, Group, AllUsers, AuthenticatedUsers |
| Grantee-ID | GtkEntry | ID oder Email (abhängig von Typ) |
| Permission-Checkboxes | GtkCheckButton | Read, Write, ReadACP, WriteACP, FullControl |
| Canned-ACL-Dropdown | GtkDropDown | private, public-read, public-read-write, etc. |

**Validierung:**

- Mindestens ein Grantee muss FullControl haben (Besitzer)
- Keine doppelten Grantee-Einträge
- Bei AllUsers: Nur Read und ReadACP erlaubt

---

### 3.8 Statusleiste

**Typ:** GtkBox (horizontal), fixiert am unteren Fensterrand

**Segmente (von links nach rechts):**

```
[🟢 Verbunden: production]  [🟡 Verbunden: staging]  [📦 5 Buckets]  [⚡ 0 Transfers]  [💾 Cache: 1.2 MB]
```

| Segment | Inhalt | Farbe |
|---------|--------|-------|
| Profil-Status | Icon + Profil-Name | 🟢=verbunden, 🟡=verbunden+Cache, 🔴=Fehler, ⚪=getrennt |
| Bucket-Count | "📦 N Buckets" | Standard |
| Transfer-Count | "⚡ N aktiv / M pausiert" | Blau bei aktiv, Gelb bei pausiert |
| Cache-Status | "💾 Cache: X MB" | Standard |
| Letzte Aktion | "Zuletzt: Bucket geladen (vor 30s)" | Grau (verschwindet nach 10s) |

---

## 4. Interaction Patterns

### 4.1 Drag & Drop

**Systemweites Drag & Drop (GTK4 DnD API):**

| Szenario | Quelle | Ziel | MIME-Type | Aktion |
|----------|--------|------|-----------|--------|
| S3→S3 Transfer | Pane A (S3) | Pane B (S3) | `application/x-r2-object` | CopyObject / Download+Upload |
| S3→Lokal Download | Pane (S3) | Dateimanager | `text/uri-list` | Download + File-Save |
| Lokal→S3 Upload | Dateimanager | Pane (S3) | `text/uri-list`, `application/x-files` | Upload |
| S3→S3 Kopie (gleiches Pane) | Pane (S3) | Selbes Pane, anderer Prefix | `application/x-r2-object` | CopyObject |
| Queue-Reorder | Queue-Eintrag | Queue (andere Position) | `application/x-r2-transfer` | Priorität ändern |

**Drag-Start (S3-Objekte):**

```
1. Benutzer selektiert 1+ Objekte
2. Mousedown + Mousemove > 5px
3. Drag-Overlay erzeugen:
   - 1 Objekt: Dateiname + Icon
   - N Objekte: "N Dateien" + erstes Icon
4. MIME-Type setzen: application/x-r2-object
5. Daten: JSON-serialisierte Vec<ObjectInfo>
```

**Drop-Target-Validierung:**

| Ziel | Gültig? | Feedback |
|------|---------|----------|
| Anderes Pane (S3) | ✅ Ja | Grüner Rahmen |
| Selbes Pane, anderer Prefix | ✅ Ja | Grüner Rahmen um Ziel-Prefix |
| Selbes Pane, gleicher Prefix | ❌ Nein | Roter Cursor |
| Pane ohne Bucket | ❌ Nein | Roter Cursor |
| Dateimanager (extern) | ✅ Ja | Standard OS-Cursor |
| Queue-Panel | ✅ Ja | Grüner Rahmen (Reorder) |

**Drop-Processing:**

```
1. Drop empfangen
2. Daten deserialisieren (ObjectInfo-Liste)
3. Ziel bestimmen (Pane, Prefix, lokaler Pfad)
4. Transfer-Typ ermitteln:
   - Quelle=S3, Ziel=S3 → S3→S3
   - Quelle=Lokal, Ziel=S3 → Upload
   - Quelle=S3, Ziel=Lokal → Download
5. Transfer-Jobs erstellen
6. Queue öffnen
7. Transfers starten
```

---

### 4.2 Context Menus

**Objekt-Context-Menu (Rechtsklick auf 1+ Objekte):**

```
┌─────────────────────────────────┐
│ 📄 Herunterladen...       Strg+D│
│ 📤 Download als...              │
│ ─────────────────────────────── │
│ 📋 Kopieren (in Zwischenablage) │
│ 📋 Pfad kopieren                │
│ 🔗 URL kopieren                 │
│ ─────────────────────────────── │
│ ✏ Umbenennen              F2   │
│ 📂 Verschieben nach...          │
│ 📋 Kopieren nach...             │
│ ─────────────────────────────── │
│ 🏷 Tags bearbeiten...           │
│ ℹ️ Eigenschaften          Strg+I│
│ ─────────────────────────────── │
│ 🗑 Löschen...              Entf │
└─────────────────────────────────┘
```

**Bucket-Context-Menu (Rechtsklick in Bucket-Liste):**

```
┌─────────────────────────────────┐
│ 📂 Bucket öffnen                │
│ 📂 In anderem Pane öffnen       │
│ ─────────────────────────────── │
│ ➕ Bucket erstellen...           │
│ 🗑 Bucket löschen...             │
│ ─────────────────────────────── │
│ 🔄 Versioning aktivieren        │
│ 🔒 ACL bearbeiten...            │
│ ℹ️ Eigenschaften                │
└─────────────────────────────────┘
```

**Pane-Context-Menu (Rechtsklick auf leeren Bereich):**

```
┌─────────────────────────────────┐
│ 🔄 Neu laden              Strg+R│
│ 📤 Upload...              Strg+U│
│ 📁 Neuen Ordner erstellen       │
│ ─────────────────────────────── │
│ 📋 Alle auswählen         Strg+A│
│ 🔍 Suchen...              Strg+F│
│ ─────────────────────────────── │
│ ⬅️ Pane schließen               │
└─────────────────────────────────┘
```

**Transfer-Queue-Context-Menu (Rechtsklick auf Eintrag):**

```
┌─────────────────────────────────┐
│ ⏸ Pause                        │
│ ▶ Fortsetzen                    │
│ ✕ Abbrechen                     │
│ ─────────────────────────────── │
│ 📂 Ziel in Pane öffnen          │
│ 📂 Quelle in Pane öffnen        │
│ ─────────────────────────────── │
│ ℹ️ Details anzeigen             │
│ 📋 Fehlerdetails kopieren       │
└─────────────────────────────────┘
```

---

### 4.3 Keyboard Shortcuts

**Globale Shortcuts:**

| Shortcut | Aktion | Kontext |
|----------|--------|---------|
| `Strg+Q` | App beenden | Global |
| `Strg+,` | Einstellungen | Global |
| `Strg+Shift+P` | Profil-Manager | Global |
| `Strg+Shift+T` | Transfer-Queue umschalten | Global |

**Pane-Shortcuts:**

| Shortcut | Aktion | Kontext |
|----------|--------|---------|
| `Tab` | Nächstes Pane fokussieren | Global |
| `Shift+Tab` | Vorheriges Pane fokussieren | Global |
| `Strg+R` | Pane refreshen | Pane fokussiert |
| `Strg+U` | Upload-Dialog öffnen | Pane fokussiert |
| `Strg+L` | Pfad-Eingabe fokussieren | Pane fokussiert |
| `Strg+F` | Suche/Filter in Objekt-Liste | Pane fokussiert |
| `Strg+A` | Alle Objekte selektieren | Objekt-Liste fokussiert |
| `Strg+D` | Selektion umkehren | Objekt-Liste fokussiert |
| `Escape` | Selektion aufheben | Objekt-Liste fokussiert |

**Objekt-Shortcuts:**

| Shortcut | Aktion | Kontext |
|----------|--------|---------|
| `Enter` | Ordner öffnen / Datei öffnen | Objekt selektiert |
| `Alt+Enter` | Eigenschaften | Objekt selektiert |
| `F2` | Umbenennen | Objekt selektiert |
| `Strg+C` | Pfad kopieren | Objekt selektiert |
| `Strg+Shift+C` | URL kopieren | Objekt selektiert |
| `Entf` | Löschen (mit Bestätigung) | Objekt(e) selektiert |
| `Strg+Entf` | Löschen ohne Bestätigung | Objekt(e) selektiert |
| `Strg+↓` | Nächstes Objekt + Aktion | Objekt-Liste fokussiert |
| `Strg+↑` | Vorheriges Objekt + Aktion | Objekt-Liste fokussiert |

**Transfer-Queue-Shortcuts:**

| Shortcut | Aktion | Kontext |
|----------|--------|---------|
| `Leertaste` | Ausgewählten Transfer pausieren/fortsetzen | Queue fokussiert |
| `Entf` | Ausgewählten Transfer abbrechen/entfernen | Queue fokussiert |
| `Strg+Shift+R` | Alle fehlgeschlagenen wiederholen | Queue fokussiert |
| `Strg+Shift+C` | Alle abgeschlossenen löschen | Queue fokussiert |

**Navigations-Shortcuts:**

| Shortcut | Aktion |
|----------|--------|
| `Alt+←` | Zurück (History) |
| `Alt+→` | Vorwärts (History) |
| `Alt+↑` | Eine Ebene höher (Parent) |
| `Strg+1` | Pane A fokussieren |
| `Strg+2` | Pane B fokussieren |
| `Strg+Shift+1` | Bucket-Liste in Pane A fokussieren |
| `Strg+Shift+2` | Bucket-Liste in Pane B fokussieren |

---

### 4.4 Inline-Editing

**Umbenennen (F2):**

```
1. Objekt selektieren → F2 drücken
2. Name-Zelle wechselt zu GtkEntry (Edit-Modus)
3. Entry zeigt aktuellen Namen, komplett selektiert
4. Benutzer editiert Namen
5. Enter → Speichern (API-Call: CopyObject + DeleteObject)
6. Escape → Abbrechen, alter Name bleibt
7. Klick außerhalb → Wie Enter (speichern)
```

**Validierung beim Umbenennen:**

- Keine Leerzeichen am Anfang/Ende
- Keine Zeichen, die in S3-Keys ungültig sind
- Kein leerer Name
- Kein Name, der bereits existiert (clientseitige Prüfung)

---

### 4.5 Pane-Resize

**Typ:** GtkPaned (horizontal) zwischen Pane A und Pane B

**Verhalten:**

| Aktion | Ergebnis |
|--------|----------|
| Griff ziehen | Linkes Pane wird breiter/schmaler, rechtes passt sich an |
| Doppelklick auf Griff | Beide Panes auf gleiche Breite (50:50) |
| Minimale Breite | 300px pro Pane |
| Maximale Breite | 80% der Fensterbreite pro Pane |
| Position speichern | Wird in Config gespeichert und beim nächsten Start wiederhergestellt |

**Visuelles Feedback:**

- Griff: 4px breit, grau (#c0bfbc)
- Hover: Griff wird blau (#3584e4)
- Drag: Griff wird dunkelblau (#1a5fb4), Cursor wechselt zu "col-resize"

---

## 5. Error / Empty / Loading States

### 5.1 Profil-Manager

**Loading States:**

| Zustand | Anzeige |
|---------|---------|
| Profile werden geladen | Spinner + "Profile werden geladen..." |
| Test Connection läuft | Button deaktiviert + Spinner + "Teste Verbindung..." |
| Secret Key wird entschlüsselt | Spinner + "Entschlüssele Zugangsdaten..." |

**Empty States:**

| Zustand | Anzeige | Aktion |
|---------|---------|--------|
| Keine Profile vorhanden | "👋 Willkommen bei r2! Erstelle dein erstes S3-Profil, um loszulegen." | [➕ Profil erstellen] |
| Keine Buckets (verbunden) | "📦 Keine Buckets gefunden. Erstelle einen neuen Bucket." | [➕ Bucket erstellen] |
| Keine Buckets (nicht verbunden) | "🔌 Nicht verbunden. Stelle eine Verbindung her, um Buckets zu sehen." | [🔌 Verbinden] |

**Error States:**

| Zustand | Anzeige | Recovery |
|---------|---------|----------|
| Config-Datei beschädigt | "⚠️ Die Konfigurationsdatei ist beschädigt. Backup unter ~/.config/r2/profiles.toml.bak" | [Config zurücksetzen] |
| libsecret nicht verfügbar | "⚠️ Der System Keyring (libsecret) ist nicht verfügbar. Credentials können nicht sicher gespeichert werden." | [Trotzdem fortfahren (unsicher)] |
| Test Connection fehlgeschlagen | "❌ Verbindung fehlgeschlagen: [Fehlermeldung]" | [Erneut versuchen] |
| Profil konnte nicht gespeichert werden | "❌ Fehler beim Speichern: [Fehlermeldung]" | [Erneut versuchen] |

---

### 5.2 Dual-Pane-Browser

**Loading States:**

| Zustand | Anzeige |
|---------|---------|
| Bucket-Liste wird geladen | Spinner in Bucket-Tree + "Buckets werden geladen..." |
| Objekt-Liste wird geladen | Spinner in Objekt-Liste + "Objekte werden geladen..." |
| Nächste Page wird geladen | Spinner in letzter Zeile der Objekt-Liste |
| Verbindung wird aufgebaut | Pane-Header: Spinner + "Verbinde..." |
| Refresh läuft | Refresh-Button rotiert (Animation) |

**Empty States:**

| Zustand | Anzeige | Aktion |
|---------|---------|--------|
| Kein Profil ausgewählt | "🔌 Wähle ein Profil aus, um Buckets zu durchsuchen." | [Profil auswählen] |
| Kein Bucket ausgewählt | "📂 Wähle einen Bucket aus der Liste links." | — |
| Bucket ist leer | "📭 Dieser Bucket ist leer. Lade Dateien hoch oder erstelle einen Ordner." | [📤 Upload] [📁 Ordner erstellen] |
| Keine Objekte in diesem Prefix | "📁 Dieser Ordner ist leer." | [📤 Hier hochladen] |
| Keine Suchergebnisse | "🔍 Keine Objekte gefunden für '[Suchbegriff]'." | [Suche zurücksetzen] |

**Error States:**

| Zustand | Anzeige | Recovery |
|---------|---------|----------|
| Verbindung verloren | "🔴 Verbindung zu [Profil] unterbrochen." | [Erneut verbinden] |
| Bucket-Liste konnte nicht geladen werden | "❌ Buckets konnten nicht geladen werden: [Fehlermeldung]" | [↻ Erneut versuchen] |
| Objekt-Liste konnte nicht geladen werden | "❌ Objekte konnten nicht geladen werden: [Fehlermeldung]" | [↻ Erneut versuchen] |
| Zugriff verweigert (HTTP 403) | "🔒 Zugriff verweigert. Überprüfe deine Zugangsdaten und Berechtigungen." | [Profil bearbeiten] |
| Bucket nicht gefunden (HTTP 404) | "❌ Bucket '[name]' existiert nicht oder wurde gelöscht." | [Buckets neu laden] |
| Timeout | "⏱️ Zeitüberschreitung bei der Anfrage. Der Endpunkt ist möglicherweise nicht erreichbar." | [↻ Erneut versuchen] |
| Region falsch | "🌍 Falsche Region. Erwartet: [erwartet], Konfiguriert: [aktuell]" | [Region korrigieren] |

**Offline/Cache States:**

| Zustand | Anzeige |
|---------|---------|
| Online, Cache aktuell | Kein Hinweis (alles normal) |
| Online, Cache veraltet | "💾 Cache: [Datum] — [↻ Aktualisieren]" |
| Offline, Cache vorhanden | "📡 Offline — Zeige gecachte Daten vom [Datum]" |
| Offline, kein Cache | "📡 Offline — Keine gecachten Daten verfügbar" |

---

### 5.3 Transfer-Queue

**Loading States:**

| Zustand | Anzeige |
|---------|---------|
| Transfer startet | "Starte Transfer..." (kurzer Spinner) |
| Multipart-Upload initialisiert | "Initialisiere Multipart-Upload..." |
| Retry (automatisch) | "Wiederholung in 5s... (Versuch 2/3)" |

**Empty States:**

| Zustand | Anzeige |
|---------|---------|
| Keine aktiven Transfers | "⚡ Keine aktiven Transfers. Ziehe Dateien zwischen Panes, um einen Transfer zu starten." |
| Keine abgeschlossenen Transfers | "✅ Noch keine Transfers abgeschlossen." |
| Keine fehlgeschlagenen Transfers | "🎉 Keine fehlgeschlagenen Transfers." |

**Error States:**

| Zustand | Anzeige | Recovery |
|---------|---------|----------|
| Transfer fehlgeschlagen (Netzwerk) | "❌ Netzwerkfehler: Verbindung zu [Endpoint] unterbrochen" | [↻ Wiederholen] |
| Transfer fehlgeschlagen (Auth) | "❌ Authentifizierungsfehler: Zugangsdaten für [Profil] ungültig" | [Profil bearbeiten] |
| Transfer fehlgeschlagen (Quota) | "❌ Speicherkontingent überschritten" | [↻ Wiederholen (nach Freigabe)] |
| Transfer fehlgeschlagen (Datei zu groß) | "❌ Datei überschreitet maximal zulässige Größe (5 TB)" | — |
| Multipart-Fehler | "❌ Multipart-Upload fehlgeschlagen: [Part X/Y]" | [↻ Wiederholen] |
| Checksum-Fehler (Could-Have) | "❌ Checksum-Prüfung fehlgeschlagen: Erwartet [X], Erhalten [Y]" | [↻ Erneut herunterladen] |

---

### 5.4 Bucket-Operationen

**Loading States:**

| Zustand | Anzeige |
|---------|---------|
| Bucket wird erstellt | Spinner + "Erstelle Bucket '[name]'..." |
| Bucket wird gelöscht | Spinner + "Lösche Bucket '[name]'..." |
| Versioning wird umgeschaltet | Spinner + "Aktualisiere Versioning..." |

**Error States:**

| Zustand | Anzeige | Recovery |
|---------|---------|----------|
| Bucket-Name bereits vergeben | "❌ Bucket-Name '[name]' ist bereits vergeben." | [Anderen Namen eingeben] |
| Bucket-Name ungültig | "❌ Ungültiger Bucket-Name. Nur Kleinbuchstaben, Zahlen, Punkte und Bindestriche erlaubt." | [Namen korrigieren] |
| Bucket nicht leer (Löschen) | "❌ Bucket '[name]' ist nicht leer. Leere den Bucket zuerst oder aktiviere 'Force Delete'." | [Bucket leeren] |
| Versioning kann nicht deaktiviert werden | "❌ Versioning kann nicht deaktiviert werden (nur suspendieren möglich)." | [Versioning suspendieren] |

---

### 5.5 ACL-Editor

**Loading States:**

| Zustand | Anzeige |
|---------|---------|
| ACL wird geladen | Spinner + "Lade ACL..." |
| ACL wird gespeichert | Spinner + "Speichere ACL..." |

**Error States:**

| Zustand | Anzeige | Recovery |
|---------|---------|----------|
| ACL kann nicht gelesen werden | "❌ ACL konnte nicht gelesen werden: [Fehlermeldung]" | [↻ Erneut versuchen] |
| ACL kann nicht gespeichert werden | "❌ ACL konnte nicht gespeichert werden: [Fehlermeldung]" | [↻ Erneut versuchen] |
| Ungültiger Grantee | "❌ Ungültiger Grantee: [ID]" | [ID korrigieren] |
| Berechtigungskonflikt | "❌ Berechtigungskonflikt: FullControl kann nicht mit anderen Berechtigungen kombiniert werden." | [Auswahl korrigieren] |

---

## 6. Accessibility Considerations

### 6.1 Tastatur-Navigation

**Prinzip:** Jede Aktion muss ohne Maus erreichbar sein.

| Anforderung | Umsetzung |
|-------------|-----------|
| Vollständige Tastatur-Navigation | Alle Widgets sind via Tab/Shift+Tab erreichbar |
| Fokus-Reihenfolge | Logisch: Toolbar → Pane A → Pane B → Queue → Statusleiste |
| Fokus-Gruppen | Arrow-Keys innerhalb von Gruppen (Objekt-Liste, Bucket-Liste) |
| Shortcut-Dokumentation | Im Menü unter "Hilfe → Tastaturkürzel" |
| Shortcut-Konflikte | Keine Überschneidungen mit GTK4/OS-Shortcuts |

**Fokus-Reihenfolge im Detail:**

```
1. Menüleiste (F10)
2. Globale Toolbar (Profil, Bucket, Pfad)
3. Pane A:
   a. Pane-Header (Profil-Dropdown, Bucket-Dropdown)
   b. Bucket-Liste (↑↓ zur Navigation)
   c. Objekt-Liste (↑↓, PageUp, PageDown, Home, End)
   d. Pane-Footer
4. Pane B: (gleiche Reihenfolge wie Pane A)
5. Transfer-Queue-Header
6. Transfer-Queue-Liste
7. Statusleiste
```

---

### 6.2 Screenreader-Unterstützung

**GTK4 Accessibility API (a11y):**

| Komponente | Rolle | Beschreibung |
|-----------|-------|-------------|
| Pane | `GTK_ACCESSIBLE_ROLE_GROUP` | "Pane A: [Profil-Name] — [Bucket-Name]" |
| Objekt-Liste | `GTK_ACCESSIBLE_ROLE_TREE_GRID` | "Objekt-Liste: 42 Einträge" |
| Objekt-Zeile | `GTK_ACCESSIBLE_ROLE_ROW` | "photo.jpg, 5 MB, image/jpeg, vor 2 Stunden" |
| Bucket-Liste | `GTK_ACCESSIBLE_ROLE_TREE` | "Bucket-Liste: 5 Buckets" |
| Transfer-Queue | `GTK_ACCESSIBLE_ROLE_LIST` | "Transfer-Queue: 3 aktiv, 2 pausiert" |
| Fortschrittsbalken | `GTK_ACCESSIBLE_ROLE_PROGRESS_BAR` | "Upload: 62 Prozent, 26 von 42 Megabyte" |
| Breadcrumb | `GTK_ACCESSIBLE_ROLE_BREADCRUMBS` | "Navigation: Bucket, images, 2024" |

**Labels und Descriptions:**

| Element | `aria-label` / GTK a11y Label |
|---------|------------------------------|
| Profil-Dropdown | "Profil auswählen — aktuell: [Name]" |
| Bucket-Dropdown | "Bucket auswählen — aktuell: [Name]" |
| Upload-Button | "Dateien hochladen" |
| Refresh-Button | "Ansicht aktualisieren" |
| Delete-Button | "Ausgewählte Objekte löschen" |
| Pane-Resize-Griff | "Pane-Größe ändern — aktuell: 50 Prozent" |
| Transfer-Pause-Button | "Transfer pausieren: [Dateiname]" |
| Transfer-Cancel-Button | "Transfer abbrechen: [Dateiname]" |

**Live-Regions:**

| Region | `aria-live` | Aktualisierung |
|--------|-------------|----------------|
| Transfer-Fortschritt | `polite` | Bei jeder Prozent-Änderung |
| Fehlermeldungen | `assertive` | Sofort |
| Statusleiste | `polite` | Bei Status-Änderung |
| Transfer abgeschlossen | `polite` | Bei Abschluss |

---

### 6.3 Farben und Kontraste

**WCAG 2.1 AA-Konformität (mindestens):**

| Anforderung | Vorgabe |
|-------------|---------|
| Normaler Text | Kontrastverhältnis ≥ 4.5:1 |
| Großer Text (>18px / >14px bold) | Kontrastverhältnis ≥ 3:1 |
| UI-Komponenten (Icons, Borders) | Kontrastverhältnis ≥ 3:1 |
| Fokus-Indikator | 2px breit, Kontrast ≥ 3:1 zum Hintergrund |

**Farbpalette (Light Theme):**

| Verwendung | Farbe | Hex | Kontrast zu Weiß |
|-----------|-------|-----|-----------------|
| Hintergrund | Weiß | #FFFFFF | — |
| Text primär | Fast Black | #1A1A1A | 17.5:1 |
| Text sekundär | Dark Gray | #5E5E5E | 7.5:1 |
| Akzent (Buttons, Links) | Blau | #3584E4 | 4.8:1 |
| Erfolg | Gr| Erfolg | Grün | #33D17A | 1.8:1 (nur als Icon-Farbe) |
| Fehler | Rot | #E01B24 | 4.6:1 |
| Warnung | Gelb | #F5C211 | 2.9:1 (nur als Icon-Farbe) |
| Resize-Griff | Grau | #C0BFBc | 2.1:1 (nur als Handle-Farbe) |

**Farbpalette (Dark Theme):**

| Verwendung | Farbe | Hex | Kontrast zu Schwarz |
|-----------|-------|-----|---------------------|
| Hintergrund | Fast Black | #1A1A1A | — |
| Text primär | Weiß | #FFFFFF | 17.5:1 |
| Text sekundär | Light Gray | #9A9996 | 4.6:1 |
| Akzent (Buttons, Links) | Hellblau | #62A0EA | 4.8:1 |
| Erfolg | Hellgrün | #8FF0A4 | 2.8:1 (nur als Icon-Farbe) |
| Fehler | Hellrot | #F66151 | 4.2:1 |
| Warnung | Hellgelb | #F8E45C | 3.1:1 (nur als Icon-Farbe) |
| Resize-Griff | Dunkelgrau | #3D3846 | 3.2:1 |

---

### 6.4 Fokus-Management

**Fokus-Indikator:**

| Anforderung | Umsetzung |
|-------------|-----------|
| Sichtbarkeit | 2px blauer outline (#3584E4) um fokussiertes Element |
| Kontrast | Kontrast ≥ 3:1 zum Hintergrund |
| Breite | Volle Breite des fokussierten Elements |
| Position | Direkt auf dem Element, keine Verschiebung |

**Fokus-Reihenfolge (Tab-Order):**

```
Widget                     → Nächstes Widget
──────────────────────────────────────────────
Menu-Leiste               → F10 (direkter Zugang)
Datei > Neu               → Enter (Aktion)
Globale Toolbar           → Tab (von links nach rechts)
Pane A Header             → Tab (Profil → Bucket → Refresh)
Pane A Bucket-Liste       → Tab/Arrow (Navigation in Liste)
Pane A Objekt-Liste       → Tab/Arrow (Navigation in Liste)
Pane B Header             → Tab (wie Pane A)
Pane B Bucket-Liste       → Tab/Arrow
Pane B Objekt-Liste       → Tab/Arrow
Transfer-Queue Header    → Tab
Transfer-Queue Einträge   → Arrow (zwischen Einträgen)
Transfer-Queue Buttons   → Tab (Pause, Cancel)
Statusleiste              → Tab (von links nach rechts)
Fenster-Buttons           → Tab ([-] [□] [×])
```

**Skip-Links:**

| Shortcut | Ziel |
|----------|------|
| `Alt+1` | Direkt zu Pane A |
| `Alt+2` | Direkt zu Pane B |
| `Alt+3` | Direkt zur Transfer-Queue |
| `Alt+4` | Direkt zur Statusleiste |

---

### 6.5 Bewegung und Animation

**Animationsrichtlinien:**

| Anforderung | Umsetzung |
|-------------|-----------|
| Minimieren | Animationen sind optional; Benutzer kann sie deaktivieren (GTK4 `gtk-enable-animations`) |
| Kurze Dauer | ≤ 200ms für UI-Feedback (Hover, Focus) |
| Mittlere Dauer | ≤ 300ms für Panel-Ein-/Ausblenden |
| Lange Dauer | ≤ 500ms für Transfer-Fortschritt (Progress) |
| Keine Ablenkung | Keine Animationen, die den Workflow unterbrechen |

**Erlaubte Animationen:**

| Animation | Dauer | Typ | Zweck |
|-----------|-------|-----|-------|
| Pane-Resize | real-time | Linear | Resize-Handle folgt Maus |
| Panel-Slide (Queue) | 300ms | Ease-out | Ein-/Ausblenden des Panels |
| Spinner | kontinuierlich | Linear | Ladezustand anzeigen |
| Refresh-Button | 500ms | Rotate | Aktualisierung läuft |
| Fortschrittsbalken | real-time | Linear | Transfer-Fortschritt |
| Hover-Fade | 150ms | Ease-in-out | Button/Link-Hover |
| Focus-Ring | 100ms | Ease-in | Fokus-Indikator erscheint |

**Verbotene Animationen:**

| Animation | Grund |
|-----------|-------|
| Scroll-Animationen (Parallax etc.) | Ablenkend, nicht barrierefrei |
| Blinkende Elemente | Auslöser für photosensitive Epilepsie |
| Autoplay-Videos | Ablenkend, nicht kontrollierbar |
| Exzessive Particle-Effekte | Ablenkend, nicht barrierefrei |

**Reduzierte Bewegung (PrefersReducedMotion):**

Bei `gtk-enable-animations = FALSE` oder `prefers-reduced-motion: reduce`:
- Alle Animationen auf 0ms setzen
- Sofortige Übergänge
- Keine Spinner, stattdessen statische Ladeindikatoren
- Transfer-Fortschritt bleibt, aber ohne Zwischenanimationen

---

> **Dokumentversion:** 1.0
> **Erstellt:** 11. Mai 2026
> **Basierend auf:** SRD.md v1.0

