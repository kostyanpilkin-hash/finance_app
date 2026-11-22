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
    version: "0.3.2",
    items: {
      ru: [
        "Журнал снова показывает понятные сводки вместо служебных ключей",
        "Записи журнала сворачиваются: клик раскрывает все детали изменения",
        "Курсы валют на дашборде — аккуратной полосой на всю ширину",
        "Карточки диаграмм больше не растягиваются с пустым местом внутри",
      ],
      be: [
        "Журнал зноў паказвае зразумелыя зводкі замест службовых ключоў",
        "Запісы журнала згортваюцца: клік раскрывае ўсе дэталі змены",
        "Курсы валют на дашбордзе — акуратнай паласой на ўсю шырыню",
        "Карткі дыяграм больш не расцягваюцца з пустым месцам унутры",
      ],
      en: [
        "The journal shows readable summaries again instead of raw keys",
        "Journal entries collapse: click to reveal every detail of the change",
        "Dashboard exchange rates now sit in a tidy full-width strip",
        "Chart cards no longer stretch with dead space inside",
      ],
    },
  },
  {
    version: "0.3.1",
    items: {
      ru: [
        "Настройки → «Устройства» теперь показывают, хост это устройство или клиент, и список подключённых",
        "Кнопка «Синхронизировать» и понятные ошибки синхронизации вместо сырого текста сети",
        "Тысячи ложных конфликтов курсов закрыты — настоящие остаются и группируются",
        "Кнопка «Обновить» на всех главных страницах подтягивает данные с хоста",
        "На дашборде — карточка курсов валют (БСБ приложение, карточки, Нацбанк), отключается в настройках",
        "Журнал: понятные сводки, автор и устройство, фильтры и местное время",
        "На Mac больше нет чёрной полосы снизу в полноэкранном режиме",
      ],
      be: [
        "Налады → «Прылады» паказваюць, хост гэтая прылада ці кліент, і спіс падключаных",
        "Кнопка «Сінхранізаваць» і зразумелыя памылкі сінхранізацыі замест сырога тэксту сеткі",
        "Тысячы ілжывых канфліктаў курсаў закрыты — сапраўдныя застаюцца і групуюцца",
        "Кнопка «Абнавіць» на ўсіх галоўных старонках падцягвае даныя з хаста",
        "На дашбордзе — картка курсаў валют (БСБ праграма, карткі, Нацбанк), адключаецца ў наладах",
        "Журнал: зразумелыя зводкі, аўтар і прылада, фільтры і мясцовы час",
        "На Mac больш няма чорнай паласы ўнізе ў поўнаэкранным рэжыме",
      ],
      en: [
        "Settings → Devices now shows whether this device is the host or a client, plus paired devices",
        "A Sync now button and human-readable sync errors instead of raw network text",
        "Thousands of false rate conflicts are closed — real ones remain and are grouped",
        "A Refresh button on every main page pulls changes from the host",
        "Dashboard gets a currency-rates card (BSB app, BSB cards, NBRB), toggleable in settings",
        "Journal: readable summaries, author and device, filters, local time",
        "No more black strip at the bottom of the window on macOS fullscreen",
      ],
    },
  },

  {
    version: "0.3.0",
    items: {
      ru: [
        "Суммы в другой валюте подсказываются по курсу БСБ Банка — как реально проводит банк",
        "Курс можно переключить в операции: БСБ приложение, БСБ карточки или Нацбанк",
        "При ручном вводе видно фактический курс и отклонение от курса банка",
        "Для рублей из России курс БСБ показывается только для справки",
        "В боте — те же подсказки по курсу БСБ и кнопка «По Нацбанку»",
        "Обновите приложение на всех устройствах — иначе синхронизация не пройдёт",
      ],
      be: [
        "Сумы ў іншай валюце падказваюцца па курсе БСБ Банка — як рэальна праводзіць банк",
        "Курс можна пераключыць у аперацыі: БСБ праграма, БСБ карткі або Нацбанк",
        "Пры ручным уводзе відаць фактычны курс і адхіленне ад курса банка",
        "Для рублёў з Расіі курс БСБ паказваецца толькі для даведкі",
        "У боце — тыя ж падказкі па курсе БСБ і кнопка «Па Нацбанку»",
        "Абнавіце праграму на ўсіх прыладах — інакш сінхранізацыя не пройдзе",
      ],
      en: [
        "Foreign-currency amounts are suggested at BSB Bank rates — the way the bank books them",
        "Switch the rate per entry: BSB app, BSB cards or NBRB",
        "Typing an amount by hand shows the actual rate and its gap to the bank rate",
        "For roubles from Russia the BSB rate is shown for reference only",
        "The bot gets the same BSB suggestions plus an “At NBRB” button",
        "Update the app on every device — otherwise sync will stop",
      ],
    },
  },
  {
    version: "0.2.0",
    items: {
      ru: [
        "Фото чеков теперь видно в карточке операции и они синхронизируются между устройствами",
        "Перевод между счетами в разных валютах спрашивает сумму зачисления и подсказывает курс",
        "Новые выпадающие списки везде: цвета счетов, иконки категорий, поиск в один ряд",
        "На Mac приложение больше не донимает паролем от связки ключей",
        "Под капотом — большая работа над надёжностью синхронизации и бэкапов",
      ],
      be: [
        "Фота чэкаў цяпер відаць у картцы аперацыі, і яны сінхранізуюцца паміж прыладамі",
        "Пераклад паміж рахункамі ў розных валютах пытае суму залічэння і падказвае курс",
        "Новыя выпадныя спісы ўсюды: колеры рахункаў, іконкі катэгорый, пошук у адзін рад",
        "На Mac праграма больш не дакучае паролем ад звязкі ключоў",
        "Пад капотам — вялікая праца над надзейнасцю сінхранізацыі і бэкапаў",
      ],
      en: [
        "Receipt photos now show inside the transaction card and sync between devices",
        "Cross-currency transfers ask for the credited amount and suggest the rate",
        "New dropdowns everywhere: account colours, category icons, one-row filters",
        "On macOS the app no longer nags for the keychain password",
        "Under the hood — a big reliability overhaul of sync and backups",
      ],
    },
  },
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
