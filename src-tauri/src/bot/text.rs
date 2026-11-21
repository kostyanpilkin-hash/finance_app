//! Bot texts (ru/be/en) and presentation helpers: money, dates, progress bars, icons.

use chrono::{Datelike, NaiveDate};
use std::collections::HashMap;
use std::sync::OnceLock;

pub(super) fn tr(lang: &str, key: &str) -> String {
    let t: (&str, &str, &str) = match key {
        "title" => ("👛 Финансы", "👛 Фінансы", "👛 Finance"),
        "welcome" => ("Выберите действие", "Абярыце дзеянне", "Choose an action"),
        "choose" => ("выберите действие", "абярыце дзеянне", "choose an action"),
        "expense" => ("💸 Расход", "💸 Выдатак", "💸 Expense"),
        "income" => ("💰 Доход", "💰 Даход", "💰 Income"),
        "transfer" => ("🔄 Перевод", "🔄 Перавод", "🔄 Transfer"),
        "balance" => ("👛 Балансы", "👛 Балансы", "👛 Balances"),
        "summary" => ("📊 Сводка", "📊 Зводка", "📊 Summary"),
        "history" => ("🧾 Операции", "🧾 Аперацыі", "🧾 Entries"),
        "goals" => ("🎯 Цели", "🎯 Мэты", "🎯 Goals"),
        "rates" => ("💱 Курсы", "💱 Курсы", "💱 Rates"),
        "undo" => ("↩️ Отменить последнюю", "↩️ Адмяніць апошнюю", "↩️ Undo last"),
        "help" => ("❓ Помощь", "❓ Дапамога", "❓ Help"),
        "menu" => ("🏠 Меню", "🏠 Меню", "🏠 Menu"),
        "back" => ("⬅️ Назад", "⬅️ Назад", "⬅️ Back"),
        "cancel" => ("✖️ Отмена", "✖️ Адмена", "✖️ Cancel"),
        "skip" => ("⏭ Пропустить", "⏭ Прапусціць", "⏭ Skip"),
        "save" => ("✅ Сохранить", "✅ Захаваць", "✅ Save"),
        "save_now" => ("⚡ Сохранить сразу", "⚡ Захаваць адразу", "⚡ Save now"),
        "edit" => ("✏️ Изменить", "✏️ Змяніць", "✏️ Edit"),
        "delete" => ("🗑 Удалить", "🗑 Выдаліць", "🗑 Delete"),
        "delete_yes" => ("🗑 Да, удалить", "🗑 Так, выдаліць", "🗑 Yes, delete"),
        "refresh" => ("🔄 Обновить", "🔄 Абнавіць", "🔄 Refresh"),
        "chart" => ("🥧 Диаграмма", "🥧 Дыяграма", "🥧 Chart"),
        "now" => ("🕐 Сейчас", "🕐 Цяпер", "🕐 Now"),
        "today" => ("☀️ Сегодня", "☀️ Сёння", "☀️ Today"),
        "yesterday" => ("🌙 Вчера", "🌙 Учора", "🌙 Yesterday"),
        "tomorrow" => ("🌅 Завтра", "🌅 Заўтра", "🌅 Tomorrow"),
        "other" => ("🗓 Другая дата", "🗓 Іншая дата", "🗓 Other date"),
        "week" => ("🗓 Неделя", "🗓 Тыдзень", "🗓 Week"),
        "month" => ("📆 Месяц", "📆 Месяц", "📆 Month"),
        "amount" => ("💵 Сумма", "💵 Сума", "💵 Amount"),
        "account" => ("💳 Счёт", "💳 Рахунак", "💳 Account"),
        "destination" => ("📥 Куда", "📥 Куды", "📥 To"),
        "credited" => ("💱 Зачислено", "💱 Залічана", "💱 Credited"),
        "category" => ("🏷 Категория", "🏷 Катэгорыя", "🏷 Category"),
        "date" => ("📅 Дата", "📅 Дата", "📅 Date"),
        "time" => ("🕐 Время", "🕐 Час", "🕐 Time"),
        "comment" => ("💬 Комментарий", "💬 Каментар", "💬 Note"),
        "photo" => ("📎 Чек", "📎 Чэк", "📎 Receipt"),
        "author" => ("👤 Автор", "👤 Аўтар", "👤 Author"),
        "spent_by" => ("👤 Кто потратил", "👤 Хто выдаткаваў", "👤 Spent by"),
        "received_by" => ("👤 Кто получил", "👤 Хто атрымаў", "👤 Received by"),
        "whose_op" => ("👤 Чья операция", "👤 Чыя аперацыя", "👤 Whose entry"),
        "created" => ("🕓 Создано", "🕓 Створана", "🕓 Created"),
        "changed" => ("✏️ Изменено", "✏️ Зменена", "✏️ Edited"),
        "source" => ("📍 Источник", "📍 Крыніца", "📍 Source"),
        "src_bot" => ("🤖 бот", "🤖 бот", "🤖 bot"),
        "src_desktop" => ("💻 приложение", "💻 праграма", "💻 app"),
        "src_reminder" => ("⏰ напоминание", "⏰ напамін", "⏰ reminder"),
        "src_other" => ("🔁 синхронизация", "🔁 сінхранізацыя", "🔁 sync"),
        "attached" => ("прикреплён", "прымацаваны", "attached"),
        "amount_hint" => ("Введите сумму, например 12,50 или 12.50", "Увядзіце суму, напрыклад 12,50 або 12.50", "Enter the amount, e.g. 12.50 or 12,50"),
        "account_from" => ("С какого счёта?", "З якога рахунку?", "From which account?"),
        "account_to" => ("На какой счёт?", "На які рахунак?", "To which account?"),
        "currency" => ("💱 Валюта", "💱 Валюта", "💱 Currency"),
        "currency_pick" => ("В какой валюте сумма?", "У якой валюце сума?", "Which currency is the amount in?"),
        "account_currency" => ("валюта счёта", "валюта рахунку", "account currency"),
        "conv_out" => ("Сколько списано со счёта в", "Колькі спісана з рахунку ў", "How much was debited from the account in"),
        "conv_in" => ("Сколько зачислено на счёт в", "Колькі залічана на рахунак у", "How much was credited to the account in"),
        "rate_bsb_app" => ("БСБ приложение", "БСБ праграма", "BSB app"),
        "rate_bsb_card" => ("БСБ по карточкам", "БСБ па картках", "BSB cards"),
        "rate_nbrb" => ("Нацбанк", "Нацбанк", "NBRB"),
        "by_nbrb" => ("По Нацбанку", "Па Нацбанку", "At NBRB"),
        "for_reference" => ("Для справки", "Для даведкі", "For reference"),
        "bsb_unavailable" => ("курс БСБ недоступен, по Нацбанку", "курс БСБ недаступны, па Нацбанку", "BSB rate unavailable, NBRB used"),
        "rates_bsb" => ("БСБ, покупка / продажа (приложение · карточки):", "БСБ, купля / продаж (праграма · карткі):", "BSB, buy / sell (app · cards):"),
        "on_account" => ("🏦 По счёту", "🏦 Па рахунку", "🏦 On account"),
        "category_pick" => ("Выберите категорию (⭐ — частые)", "Абярыце катэгорыю (⭐ — частыя)", "Choose a category (⭐ — frequent)"),
        "spender_pick" => ("Чья трата?", "Чый выдатак?", "Whose expense?"),
        "receiver_pick" => ("Кто получил?", "Хто атрымаў?", "Who received it?"),
        "whose_pick" => ("Чья операция?", "Чыя аперацыя?", "Whose entry is it?"),
        "destination_pick" => ("На какой счёт перевести?", "На які рахунак перавесці?", "Transfer to which account?"),
        "credited_hint" => ("Сколько зачислено на счёт назначения?", "Колькі залічана на рахунак прызначэння?", "How much was credited to the destination?"),
        "date_pick" => ("Когда была операция?", "Калі была аперацыя?", "When did it happen?"),
        "date_hint" => ("Введите дату ДД.ММ (текущий год), например 04.11. Можно также ДД.ММ.ГГГГ.", "Увядзіце дату ДД.ММ (бягучы год), напрыклад 04.11. Можна таксама ДД.ММ.ГГГГ.", "Enter DD.MM (current year), e.g. 04.11. DD.MM.YYYY is also supported."),
        "time_hint" => ("Нажмите «Сейчас» или введите время ЧЧ:ММ, например 14:30", "Націсніце «Цяпер» або ўвядзіце час ГГ:ХХ, напрыклад 14:30", "Tap Now or type the time as HH:MM, e.g. 14:30"),
        "comment_hint" => ("Отправьте комментарий или пропустите шаг", "Дашліце каментар або прапусціце крок", "Send a note or skip this step"),
        "photo_hint" => ("Отправьте фото чека или пропустите шаг", "Дашліце фота чэка або прапусціце крок", "Send a receipt photo or skip this step"),
        "review" => ("Проверьте и сохраните", "Праверце і захавайце", "Review and save"),
        "edit_pick" => ("Что изменить?", "Што змяніць?", "What to change?"),
        "saved" => ("✅ Операция сохранена", "✅ Аперацыя захавана", "✅ Entry saved"),
        "updated" => ("✅ Изменения сохранены", "✅ Змены захаваны", "✅ Changes saved"),
        "deleted" => ("🗑 Операция удалена", "🗑 Аперацыя выдалена", "🗑 Entry deleted"),
        "total" => ("💼 Всего", "💼 Усяго", "💼 Total"),
        "today_line" => ("☀️ Сегодня", "☀️ Сёння", "☀️ Today"),
        "month_line" => ("📆 Месяц", "📆 Месяц", "📆 Month"),
        "no_rate" => ("⚠️ Нет курса для", "⚠️ Няма курсу для", "⚠️ No rate for"),
        "cancel_question" => ("Удалить черновик операции?", "Выдаліць чарнавік аперацыі?", "Discard the draft entry?"),
        "discard" => ("🗑 Удалить черновик", "🗑 Выдаліць чарнавік", "🗑 Discard draft"),
        "continue" => ("▶️ Продолжить ввод", "▶️ Працягнуць увод", "▶️ Continue entry"),
        "stale" => ("Этот экран устарел. Используйте последнее сообщение или /menu.", "Гэты экран састарэў. Выкарыстоўвайце апошняе паведамленне або /menu.", "This screen is outdated. Use the latest message or /menu."),
        "failed" => ("⚠️ Не удалось выполнить действие. Данные сохранены в черновике — попробуйте снова.", "⚠️ Не ўдалося выканаць дзеянне. Даныя захаваны ў чарнавіку — паспрабуйце зноў.", "⚠️ Action failed. Your draft is kept — please try again."),
        "photo_failed" => ("⚠️ Не удалось загрузить чек (максимум 10 МБ). Повторите или пропустите шаг.", "⚠️ Не ўдалося загрузіць чэк (максімум 10 МБ). Паўтарыце або прапусціце крок.", "⚠️ Receipt download failed (maximum 10 MB). Retry or skip this step."),
        "empty" => ("Список пуст — добавьте записи в приложении.", "Спіс пусты — дадайце запісы ў праграме.", "Nothing here yet — add items in the app."),
        "undo_question" => ("Удалить последнюю операцию, внесённую через бот? Отмена доступна 10 минут.", "Выдаліць апошнюю аперацыю, унесеную праз бот? Адмена даступная 10 хвілін.", "Delete your last bot entry? Undo is available for 10 minutes."),
        "undo_confirm" => ("↩️ Подтвердить отмену", "↩️ Пацвердзіць адмену", "↩️ Confirm undo"),
        "undone" => ("↩️ Операция отменена", "↩️ Аперацыя адменена", "↩️ Entry undone"),
        "no_undo" => ("Нет операции, доступной для отмены", "Няма аперацыі для адмены", "No entry available to undo"),
        "help_text" => ("Выберите действие в меню или кнопками внизу экрана.\n\n💸 💰 🔄 — пошаговый ввод: сумма → счёт → категория → дата → время → комментарий → чек. Запись появляется только после «Сохранить» или «Сохранить сразу».\n🧾 Операции — последние записи обоих пользователей: просмотр, изменение, удаление.\n📊 Сводка — итоги и круговая диаграмма расходов.\n↩️ Отмена последней записи доступна 10 минут.\n\nБот работает, пока запущено приложение на основном компьютере.", "Абярыце дзеянне ў меню або кнопкамі ўнізе экрана.\n\n💸 💰 🔄 — пакрокавы ўвод: сума → рахунак → катэгорыя → дата → час → каментар → чэк. Запіс з'яўляецца толькі пасля «Захаваць» або «Захаваць адразу».\n🧾 Аперацыі — апошнія запісы абодвух карыстальнікаў: прагляд, змяненне, выдаленне.\n📊 Зводка — вынікі і кругавая дыяграма выдаткаў.\n↩️ Адмена апошняга запісу даступная 10 хвілін.\n\nБот працуе, пакуль запушчана праграма на асноўным камп'ютары.", "Pick an action in the menu or with the buttons at the bottom.\n\n💸 💰 🔄 — step-by-step entry: amount → account → category → date → time → note → receipt. Nothing is recorded until Save or Save now.\n🧾 Entries — latest records of both users: view, edit, delete.\n📊 Summary — totals and a pie chart of expenses.\n↩️ Undo of the last entry is available for 10 minutes.\n\nThe bot works while the app is running on the main computer."),
        "summary_pick" => ("Выберите период", "Абярыце перыяд", "Choose a period"),
        "income_word" => ("🟢 Доход", "🟢 Даход", "🟢 Income"),
        "expense_word" => ("🔴 Расход", "🔴 Выдатак", "🔴 Expense"),
        "net" => ("💼 Итого", "💼 Разам", "💼 Net"),
        "top" => ("Топ расходов", "Топ выдаткаў", "Top expenses"),
        "vs_prev" => ("Расходы к прошлому периоду", "Выдаткі да мінулага перыяду", "Expenses vs previous period"),
        "no_ops" => ("Операций за период нет", "Аперацый за перыяд няма", "No entries for this period"),
        "chart_caption" => ("🥧 Расходы по категориям", "🥧 Выдаткі па катэгорыях", "🥧 Expenses by category"),
        "chart_empty" => ("Нет расходов для диаграммы", "Няма выдаткаў для дыяграмы", "No expenses to chart"),
        "others" => ("Остальное", "Астатняе", "Other"),
        "history_title" => ("🧾 Последние операции", "🧾 Апошнія аперацыі", "🧾 Latest entries"),
        "hist_pick" => ("📅 По дате", "📅 Па даце", "📅 By date"),
        "hist_all" => ("✖️ Все даты", "✖️ Усе даты", "✖️ All dates"),
        "hist_date_pick" => ("Операции за какую дату?", "Аперацыі за якую дату?", "Entries for which date?"),
        "tx_title" => ("🧾 Операция", "🧾 Аперацыя", "🧾 Entry"),
        "delete_question" => ("Удалить эту операцию? Удаление будет записано в журнал.", "Выдаліць гэту аперацыю? Выдаленне будзе запісана ў журнал.", "Delete this entry? The deletion is recorded in the journal."),
        "tx_missing" => ("Операция не найдена или уже удалена", "Аперацыя не знойдзена або ўжо выдалена", "Entry not found or already deleted"),
        "goal_add" => ("➕ Пополнить", "➕ Папоўніць", "➕ Add"),
        "goal_sub" => ("➖ Снять", "➖ Зняць", "➖ Withdraw"),
        "goal_target" => ("🎯 Сумма цели", "🎯 Сума мэты", "🎯 Target"),
        "goal_rename" => ("✏️ Название", "✏️ Назва", "✏️ Rename"),
        "goal_archive" => ("📦 В архив", "📦 У архіў", "📦 Archive"),
        "goal_amount_hint" => ("Введите сумму", "Увядзіце суму", "Enter the amount"),
        "goal_name_hint" => ("Введите новое название", "Увядзіце новую назву", "Enter a new name"),
        "goal_until" => ("до", "да", "by"),
        "goal_reached" => ("🎉 Цель достигнута!", "🎉 Мэта дасягнута!", "🎉 Goal reached!"),
        "goal_saved" => ("✅ Цель обновлена", "✅ Мэта абноўлена", "✅ Goal updated"),
        "goal_archived" => ("📦 Цель перенесена в архив", "📦 Мэта перанесена ў архіў", "📦 Goal archived"),
        "rates_title" => ("💱 Курсы НБРБ", "💱 Курсы НБРБ", "💱 NBRB rates"),
        "rates_empty" => ("Курсы ещё не загружены — нажмите «Обновить».", "Курсы яшчэ не загружаны — націсніце «Абнавіць».", "No rates yet — tap Refresh."),
        "rates_on" => ("на", "на", "as of"),
        "reminder_title" => ("⏰ Напоминание о платеже", "⏰ Напамін пра плацёж", "⏰ Payment reminder"),
        "paid" => ("✅ Оплачено", "✅ Аплачана", "✅ Paid"),
        "rem_skip" => ("⏭ Пропустить", "⏭ Прапусціць", "⏭ Skip"),
        "rem_later" => ("⏰ Завтра", "⏰ Заўтра", "⏰ Tomorrow"),
        "rem_paid" => ("✅ Отмечено как оплачено", "✅ Адзначана як аплачана", "✅ Marked as paid"),
        "rem_paid_tx" => ("✅ Оплачено, операция создана", "✅ Аплачана, аперацыя створана", "✅ Paid, entry created"),
        "rem_skipped" => ("⏭ Платёж пропущен", "⏭ Плацёж прапушчаны", "⏭ Payment skipped"),
        "rem_tomorrow" => ("⏰ Напомню завтра", "⏰ Нагадаю заўтра", "⏰ I'll remind you tomorrow"),
        "rem_handled" => ("Напоминание уже обработано", "Напамін ужо апрацаваны", "Reminder already handled"),
        "due" => ("📅 Срок", "📅 Тэрмін", "📅 Due"),
        "daily_title" => ("🌙 Итоги дня", "🌙 Вынікі дня", "🌙 Daily summary"),
        "weekly_title" => ("🗓 Итоги недели", "🗓 Вынікі тыдня", "🗓 Weekly summary"),
        "monthly_title" => ("📆 Итоги месяца", "📆 Вынікі месяца", "📆 Monthly summary"),
        "keyboard_on" => ("⌨️ Быстрые кнопки — внизу экрана", "⌨️ Хуткія кнопкі — унізе экрана", "⌨️ Quick buttons are at the bottom of the screen"),
        "pair_help" => ("👛 Добро пожаловать! Откройте «Настройки → Telegram-бот» в приложении, создайте код привязки для своего пользователя и отправьте его сюда. Код действует 10 минут.", "👛 Вітаем! Адкрыйце «Налады → Telegram-бот» у праграме, стварыце код прывязкі для свайго карыстальніка і дашліце яго сюды. Код дзейнічае 10 хвілін.", "👛 Welcome! Open Settings → Telegram bot in the app, generate a pairing code for your user and send it here. The code is valid for 10 minutes."),
        "linked" => ("✅ Чат привязан", "✅ Чат прывязаны", "✅ Chat linked"),
        "badcode" => ("⚠️ Неверный или просроченный код", "⚠️ Няправільны або пратэрмінаваны код", "⚠️ Invalid or expired code"),
        "cmd_menu" => ("Главное меню", "Галоўнае меню", "Main menu"),
        "cmd_expense" => ("Новый расход", "Новы выдатак", "New expense"),
        "cmd_income" => ("Новый доход", "Новы даход", "New income"),
        "cmd_transfer" => ("Перевод между счетами", "Перавод паміж рахункамі", "Transfer between accounts"),
        "cmd_balance" => ("Балансы счетов", "Балансы рахункаў", "Account balances"),
        "cmd_summary" => ("Сводка и диаграмма", "Зводка і дыяграма", "Summary and chart"),
        "cmd_history" => ("Последние операции", "Апошнія аперацыі", "Latest entries"),
        "cmd_goals" => ("Цели", "Мэты", "Goals"),
        "cmd_rates" => ("Курсы валют", "Курсы валют", "Exchange rates"),
        "cmd_undo" => ("Отменить последнюю запись", "Адмяніць апошні запіс", "Undo the last entry"),
        "cmd_help" => ("Помощь", "Дапамога", "Help"),
        _ => ("", "", ""),
    };
    match lang {
        "be" => t.1,
        "en" => t.2,
        _ => t.0,
    }
    .into()
}

pub(super) fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// "1 234,50" with no-break-space grouping and a comma; integer math only.
pub(super) fn number(n: i64) -> String {
    let v = n.unsigned_abs();
    let int = (v / 100).to_string();
    let mut grouped = String::new();
    for (i, ch) in int.chars().enumerate() {
        if i > 0 && (int.len() - i).is_multiple_of(3) {
            grouped.push('\u{a0}');
        }
        grouped.push(ch);
    }
    format!("{}{grouped},{:02}", if n < 0 { "−" } else { "" }, v % 100)
}

pub(super) fn money(n: i64, currency: &str) -> String {
    if currency.is_empty() {
        number(n)
    } else {
        format!("{}\u{a0}{currency}", number(n))
    }
}

/// Coloured amount: 🔴 expense, 🟢 income, 🔵 transfer.
pub(super) fn signed(kind: &str, n: i64, currency: &str) -> String {
    let abs = money(n.abs(), currency);
    match kind {
        "expense" => format!("🔴 −{abs}"),
        "income" => format!("🟢 +{abs}"),
        _ => format!("🔵 {abs}"),
    }
}

pub(super) fn kind_icon(kind: &str) -> &'static str {
    match kind {
        "expense" => "🔴",
        "income" => "🟢",
        _ => "🔵",
    }
}

fn month_name(lang: &str, month: u32) -> &'static str {
    const RU: [&str; 12] = [
        "января",
        "февраля",
        "марта",
        "апреля",
        "мая",
        "июня",
        "июля",
        "августа",
        "сентября",
        "октября",
        "ноября",
        "декабря",
    ];
    const BE: [&str; 12] = [
        "студзеня",
        "лютага",
        "сакавіка",
        "красавіка",
        "мая",
        "чэрвеня",
        "ліпеня",
        "жніўня",
        "верасня",
        "кастрычніка",
        "лістапада",
        "снежня",
    ];
    const EN: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    let i = (month.clamp(1, 12) - 1) as usize;
    match lang {
        "be" => BE[i],
        "en" => EN[i],
        _ => RU[i],
    }
}

/// "Сегодня, 4 ноября" / "Вчера, 3 ноября" / "Завтра, 5 ноября", otherwise "04.11.2026".
pub(super) fn date_label(lang: &str, date: NaiveDate, today: NaiveDate) -> String {
    let rel = match (date - today).num_days() {
        0 => "today",
        -1 => "yesterday",
        1 => "tomorrow",
        _ => return date.format("%d.%m.%Y").to_string(),
    };
    let word = tr(lang, rel);
    let word = word.split_once(' ').map(|x| x.1).unwrap_or(&word);
    if lang == "en" {
        format!("{word}, {} {}", month_name(lang, date.month()), date.day())
    } else {
        format!("{word}, {} {}", date.day(), month_name(lang, date.month()))
    }
}

/// Label for an ISO date string (YYYY-MM-DD…), falling back to the raw value.
pub(super) fn iso_date_label(lang: &str, iso: &str) -> String {
    NaiveDate::parse_from_str(iso.get(..10).unwrap_or(iso), "%Y-%m-%d")
        .map(|d| date_label(lang, d, chrono::Local::now().date_naive()))
        .unwrap_or_else(|_| iso.to_string())
}

pub(super) fn short_date(iso: &str) -> String {
    NaiveDate::parse_from_str(iso.get(..10).unwrap_or(iso), "%Y-%m-%d")
        .map(|d| d.format("%d.%m").to_string())
        .unwrap_or_default()
}

pub(super) fn full_date(iso: &str) -> String {
    NaiveDate::parse_from_str(iso.get(..10).unwrap_or(iso), "%Y-%m-%d")
        .map(|d| d.format("%d.%m.%Y").to_string())
        .unwrap_or_else(|_| iso.to_string())
}

/// Local "dd.mm.yyyy hh:mm" for an RFC 3339 timestamp.
pub(super) fn local_stamp(rfc3339: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(rfc3339)
        .map(|t| {
            t.with_timezone(&chrono::Local)
                .format("%d.%m.%Y %H:%M")
                .to_string()
        })
        .unwrap_or_else(|_| rfc3339.to_string())
}

/// `▰▰▰▱▱▱` of `width` cells for done/total.
pub(super) fn progress(done: i64, total: i64, width: usize) -> String {
    let filled = if total <= 0 {
        0
    } else {
        ((done.clamp(0, total) as i128 * width as i128 + total as i128 / 2) / total as i128)
            as usize
    };
    format!(
        "{}{}",
        "▰".repeat(filled),
        "▱".repeat(width - filled.min(width))
    )
}

/// Integer percentage (rounded) of part/total.
pub(super) fn percent(part: i64, total: i64) -> i64 {
    if total <= 0 {
        return 0;
    }
    ((part as i128 * 100 + total as i128 / 2) / total as i128) as i64
}

fn icon_table() -> &'static HashMap<String, String> {
    static TABLE: OnceLock<HashMap<String, String>> = OnceLock::new();
    TABLE.get_or_init(|| {
        #[derive(serde::Deserialize)]
        struct Icon {
            id: String,
            emoji: String,
        }
        serde_json::from_str::<Vec<Icon>>(include_str!("../../../src/lib/category-icons.json"))
            .unwrap_or_default()
            .into_iter()
            .map(|i| (i.id, i.emoji))
            .collect()
    })
}

/// Emoji for a category icon id; categories without an icon get 🏷️.
pub(super) fn category_emoji(icon: &str) -> String {
    icon_table()
        .get(icon)
        .cloned()
        .unwrap_or_else(|| "🏷️".into())
}

pub(super) fn account_emoji(kind: &str) -> &'static str {
    match kind {
        "card" => "💳",
        "cash" => "💵",
        "savings" => "🏦",
        _ => "👛",
    }
}

pub(super) fn currency_flag(code: &str) -> &'static str {
    match code {
        "USD" => "🇺🇸",
        "EUR" => "🇪🇺",
        "RUB" => "🇷🇺",
        "PLN" => "🇵🇱",
        "CNY" => "🇨🇳",
        "GBP" => "🇬🇧",
        "UAH" => "🇺🇦",
        "BYN" => "🇧🇾",
        _ => "💱",
    }
}

/// Rate per BYN scaled by 1e6 → "3,2512".
pub(super) fn rate4(scaled: i64) -> String {
    let v = scaled.max(0);
    format!("{},{:04}", v / 1_000_000, (v % 1_000_000 + 50) / 100)
}

pub(super) fn date_input(text: &str) -> Option<String> {
    date_input_for_year(text, chrono::Local::now().year())
}

pub(super) fn date_input_for_year(text: &str, year: i32) -> Option<String> {
    let text = text.trim();
    let full = if text.len() == 5 {
        format!("{text}.{year:04}")
    } else {
        text.to_string()
    };
    let bytes = full.as_bytes();
    if bytes.len() != 10
        || bytes[2] != b'.'
        || bytes[5] != b'.'
        || !bytes
            .iter()
            .enumerate()
            .all(|(i, b)| i == 2 || i == 5 || b.is_ascii_digit())
    {
        return None;
    }
    NaiveDate::parse_from_str(&full, "%d.%m.%Y")
        .ok()
        .filter(|d| d.year() >= 1900 && d.year() <= 9999)
        .map(|d| d.to_string())
}

/// "14:30", "9:05", "14.30" → "14:30".
pub(super) fn time_input(text: &str) -> Option<String> {
    let (h, m) = text.trim().split_once([':', '.'])?;
    if h.is_empty() || h.len() > 2 || m.len() != 2 {
        return None;
    }
    let (h, m) = (h.parse::<u32>().ok()?, m.parse::<u32>().ok()?);
    (h < 24 && m < 60).then(|| format!("{h:02}:{m:02}"))
}

pub(super) fn relative_date(action: &str, today: NaiveDate) -> Option<String> {
    let days = match action {
        "date:today" => 0,
        "date:yesterday" => -1,
        "date:tomorrow" => 1,
        _ => return None,
    };
    today
        .checked_add_signed(chrono::Duration::days(days))
        .map(|d| d.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn money_and_signs() {
        assert_eq!(number(123456789), "1\u{a0}234\u{a0}567,89");
        assert_eq!(money(5, "BYN"), "0,05\u{a0}BYN");
        assert_eq!(money(-100000, "USD"), "−1\u{a0}000,00\u{a0}USD");
        assert_eq!(
            number(i64::MIN),
            "−92\u{a0}233\u{a0}720\u{a0}368\u{a0}547\u{a0}758,08"
        );
        assert!(signed("expense", 1250, "BYN").starts_with("🔴 −12,50"));
        assert!(signed("income", 1250, "BYN").starts_with("🟢 +12,50"));
        assert!(signed("transfer", 1250, "BYN").starts_with("🔵 12,50"));
        assert_eq!(rate4(3_251_249), "3,2512");
    }

    #[test]
    fn dates_and_times() {
        let today = NaiveDate::from_ymd_opt(2026, 11, 4).unwrap();
        assert_eq!(date_label("ru", today, today), "Сегодня, 4 ноября");
        assert_eq!(
            date_label("ru", today.pred_opt().unwrap(), today),
            "Вчера, 3 ноября"
        );
        assert_eq!(
            date_label("ru", today.succ_opt().unwrap(), today),
            "Завтра, 5 ноября"
        );
        assert_eq!(date_label("be", today, today), "Сёння, 4 лістапада");
        assert_eq!(date_label("en", today, today), "Today, November 4");
        assert_eq!(
            date_label("ru", NaiveDate::from_ymd_opt(2026, 10, 1).unwrap(), today),
            "01.10.2026"
        );
        assert_eq!(time_input("9:05").as_deref(), Some("09:05"));
        assert_eq!(time_input("14.30").as_deref(), Some("14:30"));
        for bad in ["24:00", "12:60", "1230", "12:5", ":30", "ab:cd"] {
            assert!(time_input(bad).is_none(), "{bad}");
        }
        assert_eq!(
            date_input_for_year("04.11", 2026).as_deref(),
            Some("2026-11-04")
        );
        assert!(date_input_for_year("29.02", 2026).is_none());
        assert_eq!(
            date_input_for_year("29.02.2024", 2026).as_deref(),
            Some("2024-02-29")
        );
        for text in ["31.04", "00.12", "12.13", "4.11", "04/11", "04.11.", ""] {
            assert!(date_input_for_year(text, 2026).is_none());
        }
        let new_year = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        assert_eq!(
            relative_date("date:yesterday", new_year).as_deref(),
            Some("2025-12-31")
        );
        let end = NaiveDate::from_ymd_opt(2026, 12, 31).unwrap();
        assert_eq!(
            relative_date("date:tomorrow", end).as_deref(),
            Some("2027-01-01")
        );
    }

    #[test]
    fn progress_icons_and_locales() {
        assert_eq!(progress(3, 8, 8), "▰▰▰▱▱▱▱▱");
        assert_eq!(progress(20, 10, 4), "▰▰▰▰");
        assert_eq!(progress(1, 0, 3), "▱▱▱");
        assert_eq!(percent(1, 3), 33);
        assert_eq!(category_emoji("shopping-cart"), "🛒");
        assert_eq!(category_emoji("tag"), "🏷️");
        assert!(icon_table().len() >= 60);
        for lang in ["ru", "be", "en"] {
            for key in [
                "title",
                "help_text",
                "stale",
                "save_now",
                "time_hint",
                "chart_caption",
                "paid",
                "cmd_menu",
                "goal_add",
                "rates_title",
                "keyboard_on",
            ] {
                assert!(!tr(lang, key).is_empty(), "{lang}:{key}");
            }
        }
    }
}
