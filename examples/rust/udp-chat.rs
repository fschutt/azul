//! This is a modernized example of https://habr.com/ru/post/433624/ (Russian) or
//! https://sudonull.com/post/6748-I-study-Rust-How-I-did-UDP-chat-with-Azul (English)
//!
//! The code was originally written by a Russian developer in 2018, when Azul
//! didn't have a stable API yet. I decided to port this example to show how
//! the architecture of Azul has changed from 2018 - 2021.

// Client implementation
mod client {

    use azul::prelude::*;
    use azul::widgets::*;
    use azul::str::String as AzString;
    use std::string::String;
    use std::{
        net::UdpSocket,
        sync::{Mutex, Arc},
    };
    use self::ChatDataModel::*;
    use self::CreateSocketError::*;

    // Наша модель данных
    // Для того чтобы ее можно было использовать в Azul она
    // обязательно должна реальизовать трейт Layout
    #[derive(Debug)]
    enum ChatDataModel {
        NotLoggedIn(LoginDataModel),
        LoggedIn(MessagingDataModel),
    }

    #[derive(Debug, Default)]
    struct LoginDataModel {
        // Порт который ввел пользователь. Мы будем его прослушивать нашим сокетом.
        port_input: String,
        // Адрес сервера котовый ввел пользователь. Мы будем к нему подключаться
        address_input: String,
    }

    #[derive(Debug)]
    struct MessagingDataModel {
        // Сообщение пользователя. Мы его отправим на сервер
        text_input: String,
        // Массив сообщений которые пришли с сервера
        messages: Vec<String>,
        // Сокет через который мы общаемся с сервером.
        socket: UdpSocket,
    }

    // css стили для нашего DOM
    const CUSTOM_CSS: &str = "
    .row { height: 50px; }
    .orange {
        background: linear-gradient(to bottom, #f69135, #f37335);
        font-color: white;
        border-bottom: 1px solid #8d8d8d;
    }";

    fn layout_login_screen(login_data: &LoginDataModel, app_data: RefAny) -> StyledDom {

        // Создаем кнопку с текстовой надписью Login
        let button = Button::new("Login")
            // Добавляем обработчик события для нажатия на кнопку
            .on_click(LoginController::login_pressed)
            // Преобразуем ее в обьект DOM
            .dom()
            // Добавляем ей класс row
            .with_class("row")
            // Добавляем ей css класс orange
            .with_class("orange");

        // Создаем текстовую метку с тектом Enter port to listen и css классом row
        let port_label = Label::new("Enter port to listen:")
            .dom()
            .with_class("row");

        // Создаем текстовое поле для ввода текста с текстом из свойства нашей модели и css классом row
        let port = TextInput::new(format!("{}", login_data.port_input))
            // Привязываем текстовое поле к свойству нашей DataModel
            // Это двухсторонняя привязка. Теперь редактирование TextInput автоматически изменяет
            // текст в свойстве нашей модели и обратное тоже верно.
            // Если мы изменим текст в нашей модели то измениться текст в TextInput
            .on_text_input(app_data.clone(), edit_port)
            .dom()
            .with_class("row");

        // Тоже что и для port_label
        let address_label = Label::new("Enter server address:")
            .dom()
            .with_class("row");

        // то же что и для port. Двухсторонняя привязка
        let address = TextInput::new(format!("{}", login_data.address_input))
            .on_text_input(app_data.clone(), edit_address_input) // ---------------------\
            .dom()                                                              //       |
            .with_class("row");                                                 //       |
                                                                                //       |
        // Создаем корневой DOM элемент в который помещяем наши UI элементы     //       |
        Dom::div()                                                              //       |
        .with_children(vec![                                                    //       |
            port_label,                                                         //       |
            port,                                                               //       |
            address_label,                                                      //       |
            address,                                                            //       |
            button,                                                             //       |
        ])                                                                      //       |
        .style(Css::from_string(CUSTOM_CSS))                                    //       |
    }                                                                           //       V

    extern "C" fn edit_address_input(data: &mut RefAny, _: &mut CallbackInfo, text_input: &TextInputState) -> Update {
        match data.downcast_mut::<ChatDataModel>() {
            Some(ChatDataModel::NotLoggedIn(login_data)) => {
                login_data.address_input = text_input.get_text();
                Update::DoNothing
            },
            _ => Update::DoNothing,
        }
    }


    extern "C" fn edit_port(data: &mut RefAny, _: &mut CallbackInfo, text_input: &TextInputState) -> Update {
        match data.downcast_mut::<ChatDataModel>() {
            Some(ChatDataModel::NotLoggedIn(login_data)) => {
                login_data.port_input = text_input.get_text();
                Update::DoNothing
            },
            _ => Update::DoNothing,
        }
    }

    fn layout_chat_screen(chat_data: &ChatDataModel, app_data: RefAny) -> StyledDom {

        // Создаем кнопку с тектом Send css классами row, orange и обработчиком события при ее нажатии
        let button = Button::new("Send")
            .on_click(app_data.clone(), MessagingController::send_pressed)
            .dom()
            .with_class("row")
            .with_class("orange");

        // Создаем поле для ввода текста с двухсторонней привязкой с свойству модели
        // self.messaging_model.text_input_state и css классом row
        let text = TextInput::new(chat_data.text_input.clone())
            .on_text_input(app_data.clone(), edit_chat_message_textinput)
            .dom()
            .with_class("row");

        // Добавляем тестовые метки которые отображают сообщения которые были написаны в чате
        let messages = chat_data.messages
            .iter()
            .map(|message| Dom::text(message.clone()))
            .take(50) // до 50 последних сообщений
            .collect();

        // Создаем корневой дом элемент и помещяем в него наши UI элементы
        Dom::div()
        .with_children(vec![
            messages,
            text,
            button,
        ])
    }

    extern "C" fn edit_chat_message_textinput(data: &mut RefAny, _: &mut CallbackInfo, text_input: &TextInputState) -> Update {
        match data.downcast_mut::<ChatDataModel>() {
            Some(ChatDataModel::LoggedIn(chat_data)) => {
                chat_data.text_input = text_input.get_text();
                Update::DoNothing
            },
            _ => Update::DoNothing,
        }
    }

    // Метод который создает конечный DOM и вызваеться каждый раз кода нужно перерисовать интерфейс
    extern "C" fn my_layout_func(data: &mut RefAny, _info: &mut LayoutCallbackInfo) -> StyledDom {

        let data_clone = data.clone();
        let data = match data.downcast_ref::<ChatDataModel>() {
            Some(s) => s,
            None => return StyledDom::default(), // ошибка
        };

        // Если мы уже подключены к серверу то показываем форму для отправки и
        // чтения сообщений иначе отображаем форму для подключения к серверу
        match data {
            NotLoggedIn(login_screen_data) => layout_login_screen(login_screen_data, data_clone),
            LoggedIn(chat_data) => layout_chat_screen(chat_data, data_clone),
        }
    }

    // Запускает цикл отрисовки GUI и обработки ввода пользователя
    pub fn run() {

        // Создаем приложение со стартовыми данными
        let data = ChatDataModel::LoggedIn(MessagingDataModel {
            text_input_state: String::new(),
            messages: Vec::new(),
            socket: None,
            has_new_message: false,
        });
        let app = App::new(RefAny::new(data), AppConfig::new(LayoutSolver::Default));

        //Создаем окно в котором будет отображать наше приложение
        let window = WindowCreateOptions::new(my_layout_func);

        //Запускаем приложение в этом окне
        app.run(window);
    }

    // CONTROLLER ----------------------------------------------------------------------------------

    //Таймату в милисекундах после которого будет прервана блокирующая операция чтения из сокета
    const TIMEOUT_IN_MILLIS: u64 = 2000;

    struct MessagingController { }

    impl MessagingController {

        // Метод отрабатывает когда пользователь
        // хочет оправить новое сообщение на сервер.
        extern "C" fn send_pressed(app_state: &mut RefAny, info: &mut CallbackInfo) -> Update {

            // Получаем во владение мутекс с нашей моделью данных.
            // Это блокирует поток отрисовки интерфейса до тех пор пока
            // мютекс не будет освобожден.
            let mut chat_data_model = match app_state.downcast_mut::<ChatDataModel>() {
                Some(s) => s,
                _ => return Update::DoNothing,
            };

            let mut chat_data = match &mut *chat_data_model {
                LoggedIn(chat_data) => chat_data,
                _ => return Update::DoNothing,
            };

            // Делаем копию введенного пользователем текста
            let mut message = String::new();
            // Очищаем поле ввода.
            std::mem::swap(&mut message, &mut chat_data.text_input);
            // Шана функция для отправки сообщения в сокет
            SocketService::send_to_socket(message, &data.messaging_model.socket);

            // Сообщаем фреймворку что после обработки
            // этого события нужно перерисовать интерфейс.
            Update::RefreshDom
        }
    }

    struct LoginController { }

    impl LoginController {

        // Метод отрабатывает когда пользователь хочет подключиться к серверу
        extern "C" fn login_pressed(app_state: &mut RefAny, info: &mut CallbackInfo) -> Update {

            let app_state_clone = app_state.clone();

            //Если мы уже подключены к серверу то прерываем выполнение метода сообщаем фреймворку
            // что нет необходимости перерисовывать интерфейс.
            let mut chat_data_model = match app_state.downcast_mut::<ChatDataModel>() {
                Some(s) => s,
                _ => return Update::DoNothing,
            };

            let login_data = match chat_data_model {
                NotLoggedIn(login_data) => login_data.clone(),
                _ => return Update::DoNothing,
            };

            // Утанавливаем флаг на то что пользователь уже подключился к серверу
            *chat_data_model = ChatDataModel::LoggedIn(MessagingDataModel {
                // Создаем сокет
                socket: SocketService::create_socket(&login_data.port_input, &login_data.address_input),
                .. Default::default()
            });

            //Добавляем задачу которая будет выполняться асинхронно в потоке из пула потоков фреймворка Azul
            //Обращение к мютексу с моделью данных блокриуте обновление UI до тех пор пока мюьютекс не освободиться
            let _thread_id = info.start_thread(Thread::new(app_state_clone, TasksService::read_from_socket_async));

            // Сообщаем фреймворку что после обработки этого события нужно перерисовать интерфейс
            Update::RefreshDom
        }
    }

    // Services ------------------------------------------------------------------------------------
    struct TasksService { }

    struct NewIncomingMessage(pub String);

    impl TasksService {

        // Асинхронная операция выполняющаяся в пуле потоков фреймворка azul
        extern "C" fn read_from_socket_async(
            initial_data: RefAny,
            sender: ThreadSender,
            receiver: ThreadReceiver
        ) {

            // Лочим мьютекс и получаем ссылку на сокет
            // Получаем копию сокета из нашей модели данных
            let socket = match initial_data.downcast_ref::<ChatDataModel>() {
                Some(LoggedIn(chat_data)) => SocketService::clone_socket(&chat_data.socket),
                _ => return,
            };

            loop {

                // Пытаемся прочитать данные из сокета.
                // Если не сделать копию сокета и напрямую ждать тут пока
                // прийдет сообщение из сокета который в мьютексе в нашей
                // модели денных то весь интерфейс переснанет обновляться
                // до тех пор пока мы не освободим мьютекс
                //
                // Если нам прило какоте то сообшение то изменяем нашу
                // модель данных modify делает то же что и .lock().unwrap()
                // с передачей результата в лямбду и освобождением мьютекса
                // после того как закончиться код лямбды
                if let Some(message) = SocketService::read_data(&socket) {
                    sender.send(ThreadSendMsg::WriteBack(ThreadWriteBackMsg {
                        data: RefAny::new(NewIncomingMessage(message)),
                        callback: WriteBackCallback { cb: Self::update_datamodel_main_thread }
                    }))
                } else if let Some(ThreadReceiveMsg::TerminateThread) = receiver.receive() {
                    break;
                }
            }
        }

        extern "C" fn update_datamodel_main_thread(
            app_data: &mut RefAny,
            incoming_data: &mut RefAny,
            _info: &mut CallbackInfo
        ) -> Update {

            let new_message = match incoming_data.downcast_ref::<NewIncomingMessage>() {
                Some(s) => s.0.clone(),
                _ => return Update::DoNothing,
            };

            match app_data.downcast_mut::<ChatDataModel>() {
                Some(LoggedIn(chat_data)) => {
                    chat_data.messages.push(new_message);
                    Update::RefreshDom
                },
                _ => Update::DoNothing,
            }
        }
    }

    struct SocketService { }

    // Подключаем структуру для представления отрезка времени из стандартной библиотеки
    use std::time::Duration;

    #[derive(Debug)]
    enum CreateSocketError {
        CannotBindSocket { address: String },
        CannotConnectToRemote { address: String },
        CannotSetTimeout { duration: Duration },
    }

    impl SocketService {

        // Читаем денные из сокета
        fn read_data(socket: &UdpSocket) -> Option<String> {
            //Буффер для данных которые будем считывать из сокета.
            let mut buf = [0u8; 4096];

            socket
            // Блокирующий вызов. Здесь поток выполнения останавливаеться до тех пор пока
            // не будут считанные данные или произойдет таймаут.
            .try_recv(&mut buf).ok()?
            // Получаем строку из массива байт в кодировке UTF8
            .map(|count| String::from_utf8(buf[..count].into())).ok()
        }

        // Отправляем строку в сокет
        fn send_to_socket(socket: &UdpSocket, message: &str) {
            // Преобразуем строку в байты в кодировке UTF8
            // и отправляем данные в сокет
            //
            // Запись данных в сокент не блокирующая т.е. поток
            // выполнения продолжит свою работу.
            let _ = socket.send(message.as_bytes);
        }

        fn create_socket(port: &str, server_address: &str) -> Result<UdpSocket, CreateSocketError> {

            // Считываем введенный пользователем порт и создаем на основе
            // него локальный адресс будем прослушивать
            let local_address = format!("127.0.0.1:{}", port.trim());

            // Создаем UDP сокет который считывает пакеты приходящие
            // на локальный адресс.
            let socket = UdpSocket::bind(&local_address)
            .ok_or(CannotBindSocket { address: local_address.clone() })?;

            // Считываем введенный пользователем адрес сервера
            let remote_address = server_address.trim();

            // Говорим нашему UDP сокету читать пакеты только от этого сервера
            socket.connect(remote_address)
            .ok_or(CannotConnectToRemote { address: remote_address.clone() })?;

            // Устанавливаем таймаут для операции чтения из сокета.
            // Запись в сокет происходит без ожидания т. е. мы просто пишем
            // данные и не ждем ничего а операция чтения из сокета блокирует
            // поток и ждет пока не прийдут данные которые можно считать.
            let timeout = Duration::from_millis(TIMEOUT_IN_MILLIS);

            // Если не установить таймаут то операция чтения из сокета
            // будет ждать бесконечно.
            socket.set_read_timeout(Some(timeout.clone()))
            .ok_or(CannotSetTimeout { duration: timeout })?;

            socket
        }

        // Создает копию нашего сокета для того чтобы не держать заблокированным
        // Мьютекс с нашей моделью данных
        fn clone_socket(socket: &UdpSocket) -> Option<UdpSocket> {
            socket.try_clone().ok()
        }
    }
}

// Server implementation
mod server {

    use std::{
        net::{UdpSocket, SocketAddr},
        time::Duration,
        sync::mpsc,
        thread,
    };

    const TIMEOUT_IN_MILLIS: u64 = 2000;

    fn read_line_stdin() -> Option<String> {
        let mut input = String::new();
        std::io::stdin().read_line(&mut input).ok()?;
        Some(input)
    }

    // Главная точка входа в приложение
    pub fn run() -> Option<()> {
        // Создаем сокет
        let socket = create_socket()?;

        // Создаем односторонний канал с одним отправителем сообщений
        // sx и множеством получателей rx
        let (sx, rx) = mpsc::channel();

        // Запускаем рассылку сообщений всем получателям в отдельном потоке
        start_sender_thread(rx, socket.try_clone().ok()?);

        loop {
            // Читаем данные из сокета и оправляем их в поток занимающийся
            // рассылкой сообшений клентам подключенным к серверу
            sx.send(read_data(&socket)).unwrap();
        }
    }

    // Метод для создания потока для рассылки сообщений клиентам
    fn start_sender_thread(rx: mpsc::Receiver<(Vec<u8>, SocketAddr)>, socket: UdpSocket) {

        // Запускаем новый поток. move значит что переменные переходят во владение лямбды и потока соответсвенно
        // Конкретнее наш новый поток "поглотит" переменные rx и socket
        thread::spawn(move || {

            // Коллеция адресов подключенных к нам клиентов. Всем им мы будем разсылать наши сообщения.
            // Вообще в реальном проекте надо бы сделать обработку отключения от нас клиента и удаления его
            // адресса из этого массива.
            let mut addresses = Vec::<SocketAddr>::new();
            // запускаем бесконечный цикл
            loop {

                // Читаем данные из канала. Тут поток будет заблокирован до тех пор пока не прийдут новые данные
                let (bytes, source) = match rx.recv() {
                    Ok(o) => o,
                    Err(e) => {
                        println!("ERROR: {}", e);
                        continue;
                    }
                };

                // Если такого адреса нет в нашем массиве то добавляем его туда
                if !addresses.contains(&source) {
                    println!(" {} connected to server", source);
                    addresses.push(source.clone());
                }

                // Декодируем UTF8 строку из массива байт
                let result = match String::from_utf8(bytes) {
                    Ok(o) => o.trim().to_string(),
                    Err(e) => {
                        println!("ERROR: {}", e);
                        continue;
                    }
                };

                println!("received {} from {}", result, source);

                // Создаем массив байт которые собираемся отправить всем нашим клиентам
                let message = format!("FROM: {} MESSAGE: {}", source, result);
                let data_to_send = message.as_bytes();

                // Проходим по коллецкии адресов и отправляем данные каждому.
                for address in &addresses {

                    // Операция записи в UDP сокет неблокирующая поэтому
                    // здесь метод не будет ждать пока сообщение прийдет
                    // к получателю и выполниться почти мнгновенно
                    if let Err(e) = socket.send_to(data_to_send, s) {
                        println!("can't send to {}", source);
                    }
                }
            }
        });
    }

    // Создает сокет на основе данных введенных пользователем
    fn create_socket() -> Option<UdpSocket> {

        println!("Enter port to listen:");
        let local_port = read_line_stdin();

        // Считываем порт который будет слушать наш сервер и создаем на его основе адрес сервера
        let local_address = format!("127.0.0.1:{}", local_port.trim());
        println!("server address {}", &local_address);

        // Создаем UDP сокет прослущивающий этот адрес
        let socket = UdpSocket::bind(&local_address.trim()).ok()?;

        // Устанавливаем таймаут для операции чтения. Операция чтения блокирующая и она заблокирует поток
        // до тех пор пока не прийдут новые данные или не наступит таймаут
        socket.set_read_timeout(Some(Duration::from_millis(TIMEOUT_IN_MILLIS))).ok()?;

        // Возвращаем из метода созданные сокет
        socket
    }

    //Читает данные из сокета и возвшает их вместе с адресом оправителя
    fn read_data(socket: &UdpSocket) -> (Vec<u8>, SocketAddr) {
        // Буфер куда будем считывать данные
        let mut buf = [0u8; 4096];
        // Запускает цикл который будет выполняться
        // до тех пор пока не будут считаны валидные данные
        loop {
            match socket.recv_from(&mut buf) {
                // Получаем количество считанных байт и адрес отправителя
                Ok((count, address)) => {
                    //Делем срез массива от его начала до количеств считанных байт и преборазуем его в вектор байт
                    return (buf[..count].into(), address);
                }
                // Если произошёл таймаут или другая ошибка то переходим к следующей итерации цикла
                Err(e) => {
                    println!("Error {}", e);
                    continue;
                }
            };
        }
    }
}

fn main() {
    #[cfg(not(feature = "server"))]
    crate::client::run();

    #[cfg(feature = "server")]
    crate::server::run();
}