# AzPdf's words in English (src/l10n.rs). Every key the source names is here and in de.ftl.

## The window

azpdf-about-summary = A PDF viewer: page thumbnails, the outline, zoom and search. azul reads the PDF and draws every page through its SVG renderer. The recent documents are kept in your data folder.
azpdf-opening-empty = Opening…
azpdf-could-not-open = The PDF could not be opened
azpdf-no-document = No document open
azpdf-no-document-detail = Open a PDF, drop one on this window, or pick a recent document.
azpdf-open-pdf-action = Open PDF…
azpdf-recent = Recent
azpdf-recent-page = p. { $page } of { $count }

## The toolbar

azpdf-document = Document
azpdf-open = Open
azpdf-previous-page = Previous page
azpdf-next-page = Next page
azpdf-first-page-now = This is the first page
azpdf-last-page-now = This is the last page
azpdf-page = Page
azpdf-page-count-name = Page count
azpdf-zoom = Zoom
azpdf-zoom-in = Zoom in
azpdf-zoom-out = Zoom out
azpdf-fit-width = Fit width
azpdf-fit-page = Fit page
azpdf-find = Find in document
azpdf-settings = Settings
azpdf-export-filled = Export filled PDF

## The pages, the outline, the search

azpdf-pages = Pages
azpdf-outline = Outline
azpdf-thumbnails = Page thumbnails
azpdf-no-outline = This document has no outline.
azpdf-page-n = Page { $page }
azpdf-searching = Searching…
azpdf-no-matches = No matches
azpdf-matches =
    { $count ->
        [one] 1 match
       *[other] { $count } matches
    }
azpdf-no-matches-for = No matches for “{ $query }”
azpdf-close-search = Close the search
azpdf-and-more = … and { $count } more
azpdf-landscape = landscape

## Shortcuts

azpdf-open-pdf = Open a PDF
azpdf-shortcuts-view = View
azpdf-shortcuts-search = Search
azpdf-shortcut-actual-size = Actual size (100 %)
azpdf-shortcut-next-previous = The next / previous page
azpdf-shortcut-first-last = The first / last page
azpdf-shortcut-find = Find in the document

## Notices

azpdf-making-sample = Making the sample document…
azpdf-opening = Opening { $title }…
azpdf-page-count =
    { $count ->
        [one] 1 page
       *[other] { $count } pages
    }
azpdf-pages-skipped =
    { $count ->
        [one] 1 page — { $skipped } parts of the file were skipped
       *[other] { $count } pages — { $skipped } parts of the file were skipped
    }
azpdf-not-opened = { $title } could not be opened
azpdf-saved = Saved { $name }
azpdf-not-filled = The form could not be filled: { $why }
azpdf-sample-not-made = The sample could not be made: { $why }
azpdf-recent-not-saved = The recent list could not be saved: { $why }
azpdf-sample-not-saved = The sample could not be saved: { $why }
azpdf-pdf-documents = PDF documents
azpdf-gone = { $title } is no longer there
azpdf-reading-text = Reading the pages’ text…
azpdf-filling-form = Filling in the form…
