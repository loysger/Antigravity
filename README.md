# 🌌 Google Antigravity IDE & CLI — Настольный клиент и шлюз прямого доступа (v1.0.37)

[![GitHub releases](https://img.shields.io/github/v/release/loysger/AGClientPub?style=flat-square&label=Версия)](https://github.com/loysger/AGClientPub/releases)
[![Downloads](https://img.shields.io/github/downloads/loysger/AGClientPub/total?style=flat-square&label=Скачиваний)](https://github.com/loysger/AGClientPub/releases)
[![Platforms](https://img.shields.io/badge/Платформы-Windows%20|%20macOS%20|%20Linux-blue?style=flat-square)](https://github.com/loysger/AGClientPub/releases)
[![Telegram](https://img.shields.io/badge/Telegram-Поддержка-blue?style=flat-square&logo=telegram)](https://t.me/Ultimateadvansed)

**Google Antigravity** — передовая среда разработки и автономный ИИ-агент нового поколения от Google. Платформа объединяет возможности **Antigravity IDE**, оркестратора **Antigravity 2.0** и консольного терминального агента **Antigravity CLI (agy)**, используя флагманские модели **Claude Sonnet 4.6**, **Claude Opus 4.6**, **Gemini 3.8 Flash** и **Gemini 3.1 Pro** для автономного написания, анализа и рефакторинга кода.

Из-за региональных ограничений прямое обращение к Google API из России и стран СНГ блокируется (возникают ошибки `400 Bad Request: User location is not supported`, `Your current account is not eligible for Antigravity, because it is not currently available in your location` или сбои выполнения агента `agent terminated due to error`).

**Antigravity Client** — специализированный кроссплатформенный шлюз, который подключает IDE напрямую к стабильному пулу мощностей: **без личного Google-аккаунта, без риска слёта подписок и без общего VPN на компьютере**.

---

## 🌟 Ключевые возможности (Features)

* **🚀 Прямой доступ без личного Google-аккаунта:** Больше не нужно регистрировать иностранные почты, искать зарубежные карты и бояться внезапного бана или слёта платной подписки.
* **⚡ Работа без сторонних VPN:** Клиент работает как локальный точечный шлюз (`127.0.0.1:8047`) только для трафика IDE. Ваш браузер, Telegram, стриминг и игры летают на полной скорости домашнего интернета.
* **🔄 Одновременный запуск IDE, 2.0 и CLI:** Возможность параллельно запускать и работать в Antigravity IDE, Antigravity 2.0 и терминальном CLI через единый общий шлюз без конфликтов портов.
* **🛠 Решение критических ошибок агента:** Устраняет ошибки `agent terminated due to error` и `agent execution terminated` за счет активного удержания WebSocket-сессий без сброса контекста.
* **🧠 Полный спектр флагманских моделей:** Доступ к Claude 4.6 (Sonnet / Opus) и Gemini 3.1 Pro с рекордным контекстом до **2 000 000 токенов**.
* **💻 Кроссплатформенность:** Официальные сборки для **Windows (exe/msi)**, **macOS (dmg для Apple Silicon & Intel)** и **Linux (AppImage/deb/rpm/WSL)**.
* **🧩 Поддержка MCP (Model Context Protocol):** Интеграция сторонних инструментов, баз данных и внешних навыков.

---

## ⚡ Технология Prompt Caching (Умное кэширование контекста)

В **Antigravity Client** реализована профессиональная технология **Prompt Caching**:
1. **Экономия лимитов в 3–5 раз:** При повторных запросах в рамках одного диалога закэшированная кодовая база считывается из быстрой памяти без расхода квоты.
2. **Увеличенный ресурс тарифа:** За счёт кэширования базовый тариф живет в несколько раз дольше обычного официального тарифа Google.
3. **Ускорение генерации:** Ответы на вопросы по структуре проекта формируются на **40–60% быстрее**.

---

## 📥 Скачать Antigravity Client (v1.0.37)

Выберите готовый установщик для вашей операционной системы:

| Операционная система | Тип пакета | Ссылка на скачивание |
|---|---|:---:|
| 🪟 **Windows (10/11 x64)** | Setup EXE (быстрая установка) | [**Скачать .exe**](https://github.com/loysger/AGClientPub/releases/download/v1.0.37/Antigravity.Client_1.0.37_x64-setup.exe) |
| 🪟 **Windows (10/11 x64)** | MSI Installer (системный пакет) | [**Скачать .msi**](https://github.com/loysger/AGClientPub/releases/download/v1.0.37/Antigravity.Client_1.0.37_x64_en-US.msi) |
| 🍎 **macOS (Apple Silicon M1/M2/M3/M4)** | DMG образ | [**Скачать .dmg (ARM64)**](https://github.com/loysger/AGClientPub/releases/download/v1.0.37/Antigravity.Client_1.0.37_aarch64.dmg) |
| 🍏 **macOS (Intel x86_64)** | DMG образ | [**Скачать .dmg (Intel)**](https://github.com/loysger/AGClientPub/releases/download/v1.0.37/Antigravity.Client_1.0.37_x64.dmg) |
| 🐧 **Linux (Ubuntu / Debian x64)** | DEB пакет | [**Скачать .deb**](https://github.com/loysger/AGClientPub/releases/download/v1.0.37/Antigravity.Client_1.0.37_amd64.deb) |
| 🐧 **Linux (любые дистрибутивы)** | Портативный AppImage | [**Скачать .AppImage**](https://github.com/loysger/AGClientPub/releases/download/v1.0.37/Antigravity.Client_1.0.37_amd64.AppImage) |
| 🐧 **Linux (Fedora / RHEL / CentOS)** | RPM пакет | [**Скачать .rpm**](https://github.com/loysger/AGClientPub/releases/download/v1.0.37/Antigravity.Client-1.0.37-1.x86_64.rpm) |

📦 Все сборки доступны на [**Странице релизов (Releases)**](https://github.com/loysger/AGClientPub/releases).

---

## 🚀 Быстрый старт за 3 шага

1. **Скачайте установщик** под вашу ОС и установите приложение.
2. **Запустите Antigravity Client** и введите персональный ключ доступа (`IMPULS-xxxxx`).
3. Нажмите кнопку **«Подключиться»** (Connect) и запустите **Antigravity IDE**, **Antigravity 2.0** или консольный агент **AGY CLI**.

> 💡 **Для пользователей macOS**:  
> Если Gatekeeper выводит предупреждение о неизвестном разработчике при первом запуске, выполните в Терминале:  
> ```bash
> xattr -cr "/Applications/Antigravity Client.app"
> ```

---

## 🧠 Поддерживаемые модели искусственного интеллекта

| Модель | Назначение |
|---|---|
| **Claude Opus 4.6 & Thinking** | Сложная архитектура, комплексные расчеты, глубокий рефакторинг |
| **Claude Sonnet 4.6 & Thinking** | Лучший автономный агент для написания и отладки кода |
| **Gemini 3.1 Pro** | Контекстное окно до **2 000 000 токенов** (весь репозиторий целиком) |
| **Gemini 3.8 Flash (High / Thinking)** | Сверхбыстрый отклик, автокомплит и мгновенный поиск багов |
| **GPT-OSS 120B** | Открытая флагманская модель для типовых задач |

---

## 🔌 Интеграция с Claude Code, Cursor, Windsurf и CLI

Локальный шлюз клиента (`127.0.0.1:8047`) совместим с любыми внешними AI-утилитами:

**Windows (PowerShell):**
```powershell
$env:HTTP_PROXY="http://127.0.0.1:8047"
$env:HTTPS_PROXY="http://127.0.0.1:8047"
```

**macOS / Linux (Bash / Zsh):**
```bash
export HTTP_PROXY="http://127.0.0.1:8047"
export HTTPS_PROXY="http://127.0.0.1:8047"
```

---

## ❓ Часто задаваемые вопросы (FAQ & База знаний)

### ❓ Что делать, если Google Antigravity не работает в России?
Прямой доступ к API Google Cloud Code заблокирован для российских IP-адресов. Antigravity Client перенаправляет только запросы IDE через выделенный локальный прокси (`127.0.0.1:8047`), не снижая общую скорость интернета на вашем компьютере.

### ❓ Как исправить ошибку «agent terminated due to error» в Antigravity IDE & 2.0?
Эта ошибка возникает из-за микроразрывов соединения при использовании обычных VPN или при исчерпании лимитов бесплатного Google-аккаунта. Подробный пошаговый разбор с логами смотрите в [**Issue #1**](https://github.com/loysger/AGClientPub/issues/1).

### ❓ Что делать при ошибке «agent execution terminated due to error» в CLI / терминале?
Длинная сигнатура ошибки со словом `execution` возникает при падении фонового раннера языкового сервера или разрыве стрима `streamGenerateContent`. Подробное решение разобрано в [**Issue #2**](https://github.com/loysger/AGClientPub/issues/2).

### ❓ Как решить ошибку «Your current account is not eligible for Antigravity, because it is not currently available in your location»?
Google проверяет постоянную страну профиля вашего Google-аккаунта (`ineligibleTiers`) и блокирует авторизацию для пользователей из РФ и СНГ даже под VPN. Подробная инструкция по обходу блокировки без смены страны профиля приведена в [**Issue #3**](https://github.com/loysger/AGClientPub/issues/3).

---

## 🤝 Обратная связь и техническая поддержка

* 📩 **Telegram (получение и продление ключей, тарифы PRO / X3 / X5 / ULTRA безлимит)**: **[@Ultimateadvansed](https://t.me/Ultimateadvansed)**
* 🐛 **Сообщить о проблеме**: [GitHub Issues](https://github.com/loysger/AGClientPub/issues)
