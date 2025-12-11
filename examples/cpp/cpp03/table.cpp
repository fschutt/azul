// Azul C++ Table Example - C++03 compatible
// Demonstrates table/grid layout with scrollable data

#include <azul.h>
#include <cstdio>
#include <cstring>

// Global string constants
static const AzString WINDOW_TITLE = AzString_fromConstStr("Azul Table - C++03");
static const AzString HEADER_STYLE = AzString_fromConstStr("font-weight: bold; background: #4a90d9; color: white; padding: 8px; border: 1px solid #2171b5;");
static const AzString CELL_STYLE = AzString_fromConstStr("padding: 6px; border: 1px solid #ccc;");
static const AzString ROW_STYLE = AzString_fromConstStr("flex-direction: row;");
static const AzString ROW_EVEN_STYLE = AzString_fromConstStr("flex-direction: row; background: #f0f0f0;");
static const AzString ROW_ODD_STYLE = AzString_fromConstStr("flex-direction: row; background: white;");
static const AzString CONTAINER_STYLE = AzString_fromConstStr("flex-grow: 1; overflow: scroll;");

// Table data
typedef struct {
    int id;
    const char* name;
    const char* email;
    int age;
} TableRow;

typedef struct {
    TableRow* rows;
    size_t row_count;
} TableData;

// Type ID for RefAny
AZ_REFLECT(TableData, TableData_destructor)

void TableData_destructor(TableData* data) {
    // In real code, would free the rows array
    (void)data;
}

// Create a cell with text
AzDom cell(const char* text, AzString style) {
    AzString content = AzString_copyFromBytes((const uint8_t*)text, strlen(text));
    AzDom c = AzDom_text(AzLabel_new(content), style);
    return c;
}

// Create a cell with integer
AzDom int_cell(int value, AzString style) {
    char buf[32];
    snprintf(buf, sizeof(buf), "%d", value);
    return cell(buf, style);
}

// Layout function
AzStyledDom layout_table(AzRefAny* state, AzLayoutCallbackInfo* info) {
    AzDom root = AzDom_div();
    AzDom_setInlineStyle(&root, CONTAINER_STYLE);
    
    // Header row
    AzDom header = AzDom_div();
    AzDom_setInlineStyle(&header, ROW_STYLE);
    AzDom_addChild(&header, cell("ID", HEADER_STYLE));
    AzDom_addChild(&header, cell("Name", HEADER_STYLE));
    AzDom_addChild(&header, cell("Email", HEADER_STYLE));
    AzDom_addChild(&header, cell("Age", HEADER_STYLE));
    AzDom_addChild(&root, header);
    
    // Data rows
    TableData data;
    if (TableData_downcastRef(state, &data)) {
        size_t i;
        for (i = 0; i < data.row_count; i++) {
            AzString row_style = (i % 2 == 0) ? ROW_EVEN_STYLE : ROW_ODD_STYLE;
            
            AzDom row = AzDom_div();
            AzDom_setInlineStyle(&row, row_style);
            
            AzDom_addChild(&row, int_cell(data.rows[i].id, CELL_STYLE));
            AzDom_addChild(&row, cell(data.rows[i].name, CELL_STYLE));
            AzDom_addChild(&row, cell(data.rows[i].email, CELL_STYLE));
            AzDom_addChild(&row, int_cell(data.rows[i].age, CELL_STYLE));
            
            AzDom_addChild(&root, row);
        }
    }
    
    return AzStyledDom_fromDom(root, AzCss_empty());
}

int main() {
    // Sample data
    static TableRow sample_rows[] = {
        {1, "Alice Johnson", "alice@example.com", 28},
        {2, "Bob Smith", "bob@example.com", 34},
        {3, "Carol White", "carol@example.com", 45},
        {4, "David Brown", "david@example.com", 23},
        {5, "Eve Davis", "eve@example.com", 31},
        {6, "Frank Miller", "frank@example.com", 52},
        {7, "Grace Lee", "grace@example.com", 27},
        {8, "Henry Wilson", "henry@example.com", 39},
        {9, "Ivy Chen", "ivy@example.com", 24},
        {10, "Jack Taylor", "jack@example.com", 41}
    };
    
    TableData initial_data;
    initial_data.rows = sample_rows;
    initial_data.row_count = sizeof(sample_rows) / sizeof(sample_rows[0]);
    
    AzRefAny state = TableData_upcast(&initial_data);
    AzLayoutCallback layout = AzLayoutCallback_new(state, layout_table);
    
    // Create app and window
    AzApp app = AzApp_new(layout);
    AzWindowCreateOptions window_opts = AzWindowCreateOptions_default();
    AzWindowCreateOptions_setTitle(&window_opts, WINDOW_TITLE);
    AzWindowCreateOptions_setDimensions(&window_opts, (AzLayoutSize){700, 400});
    
    AzApp_run(&app, window_opts);
    
    return 0;
}
