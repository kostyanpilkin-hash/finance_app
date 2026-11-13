/** Local (per-device) setting: the app version whose notes were already shown. */
export const WHATSNEW_SEEN_KEY = "whatsnew_seen";

/** Highlights for the "What's new" dialog after an update, newest first.
 *  Fill in alongside CHANGELOG.md at every release — 3–7 user-facing bullets. */
export interface WhatsNewEntry {
  version: string;
  items: Record<"ru" | "be" | "en", string[]>;
}

export const WHATS_NEW: WhatsNewEntry[] = [
  {
    version: "0.1.10",
    items: {
      ru: [
        "«Чья операция» теперь у доходов и переводов тоже, а не только у расходов",
        "В списке операций видно время, кто внёс запись и чья это операция",
        "Аналитика «по людям» считает доходы по владельцу операции",
        "В Telegram-боте выбор «чья операция» появился у всех типов",
        "В отчётах колонка «Кто потратил» стала «Чья операция» и заполняется всегда",
        "После обновления приложение показывает это окно с новинками",
      ],
      be: [
        "«Чыя аперацыя» цяпер і ў даходаў, і ў пераводаў, а не толькі ў выдаткаў",
        "У спісе аперацый відаць час, хто ўнёс запіс і чая гэта аперацыя",
        "Аналітыка «па людзях» лічыць даходы па ўладальніку аперацыі",
        "У Telegram-ботаві выбар «чыя аперацыя» з'явіўся ва ўсіх тыпаў",
        "У справаздачах слупок «Хто выдаткаваў» стаў «Чыя аперацыя» і запаўняецца заўсёды",
        "Пасля абнаўлення праграма паказвае гэта акно з навінамі",
      ],
      en: [
        "“Whose entry” now works for income and transfers too, not just expenses",
        "The transaction list shows the time, who entered it and whose entry it is",
        "“By people” analytics groups income by the entry owner",
        "The Telegram bot asks “whose entry” for every kind now",
        "The reports column “Spent by” became “Whose entry” and is always filled",
        "After an update the app shows this window with the highlights",
      ],
    },
  },
];

export function whatsNewFor(version: string): WhatsNewEntry | undefined {
  return WHATS_NEW.find((e) => e.version === version);
}
