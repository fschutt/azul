extern crate azul;

const TEST_XML: &str = "

<component name='start-screen'>
    <div id='start_screen'>
        <div id='last_projects_column'>
             <p id='last_projects_header'>LAST PROJECTS</p>
             <div id='project_btn_container'>
                <p id='new_project_btn' onleftmouseup='menu_new_project'>+</p>
                <p id='open_project_btn' onleftmouseup='menu_open_project'>Open project</p>
             </div>
        </div>
        <div id='map_preview_container'>
            <div id='map_preview' />
            <div id='map_details_view' />
        </div>
    </div>
</component>

<component name='toolbar' fn='render_calendar'>
    <div class='hello'><p>selectedDate</p></div>
</component>

<app>
    <div id='start_screen_wrapper'>
        <start-screen />
    </div>
    <calendar
        selectedDate='01.01.2018'
        minimumDate='01.01.1970'
        maximumDate='31.12.2034'
        firstDayOfWeek='sunday'
        gridVisible='false'
        dateSelectable='true'
        horizontalHeaderFormat='Mon'
        verticalHeaderFormat='S'
        navigationBarVisible='true'
    />
    <form id='test_form'>
        <section id='my_test_section'>
            <textinput placeholder='Type here...' />
        </section>
    </form>
</app>
";

/*
    element start: component
    attribute: name - start-screen
    element start: div
    attribute: id - start_screen
    element start: div
    attribute: id - last_projects_column
    element start: p
    attribute: id - last_projects_header
    text: LAST PROJECTS
    element end: p
    element start: div
    attribute: id - project_btn_container
    element start: p
    attribute: id - new_project_btn
    attribute: onleftmouseup - menu_new_project
    text: +
    element end: p
    element start: p
    attribute: id - open_project_btn
    attribute: onleftmouseup - menu_open_project
    text: Open project
    element end: p
    element end: div
    element end: div
    element start: div
    attribute: id - map_preview_container
    element start: div
    attribute: id - map_preview
    element />
    element start: div
    attribute: id - map_details_view
    element />
    element end: div
    element end: div
    element end: component
    element start: component
    attribute: name - toolbar
    attribute: fn - render_calendar
    element start: div
    attribute: class - hello
    element start: p
    text: selectedDate
    element end: p
    element end: div
    element end: component
    element start: app
    element start: div
    attribute: id - start_screen_wrapper
    element start: start-screen
    element />
    element end: div
    element start: calendar
    attribute: selectedDate - 01.01.2018
    attribute: minimumDate - 01.01.1970
    attribute: maximumDate - 31.12.2034
    attribute: firstDayOfWeek - sunday
    attribute: gridVisible - false
    attribute: dateSelectable - true
    attribute: horizontalHeaderFormat - Mon
    attribute: verticalHeaderFormat - S
    attribute: navigationBarVisible - true
    element />
    element start: form
    attribute: id - test_form
    element start: section
    attribute: id - my_test_section
    element start: textinput
    attribute: placeholder - Type here...
    element />
    element end: section
    element end: form
    element end: app
*/

fn main() {
    azul::xml::parse_xml(TEST_XML).unwrap();
}