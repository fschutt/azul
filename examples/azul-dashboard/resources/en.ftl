# AzDashboard's words in English (src/l10n.rs). Every key the source names is here and in de.ftl;
# the column titles are said by their English names (the last section).

## The window

azdashboard-about-summary = Half a million orders in azul's virtualized DataTable - sorted, filtered and edited without blocking the window - and charts of the same data. Part of the Azlin apps, built with azul.
azdashboard-orders = Orders
azdashboard-orders-count = { $rows } orders
azdashboard-sorting = Sorting...
azdashboard-showing = Showing { $rows }
azdashboard-unsorted = Unsorted
azdashboard-sorted-by = Sorted by { $keys }
azdashboard-clear-filters = Clear filters
azdashboard-clear-sort = Clear sort
azdashboard-generating = Generating { $rows } orders...
azdashboard-generating-detail = The same orders every time, on a background thread.
azdashboard-not-loaded = The orders are not loaded yet.
azdashboard-copied-rows =
    { $count ->
        [one] Copied 1 row
       *[other] Copied { $rows } rows
    }

## Editing

azdashboard-no-such-order = There is no such order.
azdashboard-not-a-date = "{ $text }" is not a date (YYYY-MM-DD).
azdashboard-priority-one-of = The priority is one of { $choices }.
azdashboard-quantity-range = The quantity is a whole number from 1 to 999.
azdashboard-unit-price-range = The unit price is an amount from 0.01 to 99,999.99.
azdashboard-discount-range = The discount is a whole percentage from 0 to 90.
azdashboard-returned-yes-no = Returned is Yes or No.
azdashboard-rating-range = The rating is 1, 2, 3, 4 or 5.
azdashboard-not-editable = { $column } is not editable.
azdashboard-this-column = This column

## The charts

azdashboard-other = Other
azdashboard-picked-row = Row { $row }: { $column } { $value }
azdashboard-picked-bar = { $name }: { $column } { $total } in total
azdashboard-charted =
    { $count ->
        [one] 1 row charted. Click a point or a bar (or Tab to a chart and press Enter) to pick it.
       *[other] { $count } rows charted. Click a point or a bar (or Tab to a chart and press Enter) to pick it.
    }
azdashboard-by-row = { $column } by row
azdashboard-row-axis = Row (table order)
azdashboard-by-category = { $column } by { $category }
azdashboard-total-of = Total { $column }

## Shortcuts

azdashboard-group-table = Table
azdashboard-group-edit = Edit
azdashboard-group-sort = Sort
azdashboard-group-filter = Filter
azdashboard-shortcut-move = Move the cell cursor
azdashboard-shortcut-extend = Extend the row selection
azdashboard-shortcut-screen = Move by a screen of rows
azdashboard-shortcut-first-last-column = The first / last column
azdashboard-shortcut-first-last-row = The first / last row
azdashboard-shortcut-select-all = Select every row shown
azdashboard-shortcut-copy = Copy the selected rows
azdashboard-shortcut-edit = Edit the cell
azdashboard-shortcut-keep = Keep the edit (Tab moves right)
azdashboard-shortcut-cancel = Cancel the edit
azdashboard-shortcut-sort = Sort by a column, add a second key
azdashboard-shortcut-filter = Filter the cursor's column

## The column titles (the data set's English names)
azdashboard-col-order = Order
azdashboard-col-order-date = Order date
azdashboard-col-ship-date = Ship date
azdashboard-col-region = Region
azdashboard-col-country = Country
azdashboard-col-city = City
azdashboard-col-customer = Customer
azdashboard-col-segment = Segment
azdashboard-col-category = Category
azdashboard-col-sub-category = Sub-category
azdashboard-col-product = Product
azdashboard-col-ship-mode = Ship mode
azdashboard-col-priority = Priority
azdashboard-col-quantity = Quantity
azdashboard-col-unit-price = Unit price
azdashboard-col-discount = Discount %
azdashboard-col-sales = Sales
azdashboard-col-cost = Cost
azdashboard-col-shipping = Shipping
azdashboard-col-profit = Profit
azdashboard-col-margin = Margin %
azdashboard-col-days-to-ship = Days to ship
azdashboard-col-returned = Returned
azdashboard-col-rating = Rating
azdashboard-col-sales-rep = Sales rep
