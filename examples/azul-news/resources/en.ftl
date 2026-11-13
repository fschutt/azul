# AzNews' words in English (src/l10n.rs). Every key the source names is here and in de.ftl.

## The window

aznews-about-summary = Your feeds - RSS, Atom and JSON Feed - in folders, refreshed politely (ETag, Last-Modified), read in a clean reader view. The list is an OPML file, the articles and your marks are plain files. Part of the Azlin apps, built with azul.
aznews-articles = Articles
aznews-sources = Sources
aznews-feeds = Feeds
aznews-panes = Panes

## The toolbar

aznews-get-news = Get News
aznews-fetching = Fetching…
aznews-mark-read = Mark Read
aznews-mark-unread = Mark Unread
aznews-star = Star
aznews-unstar = Unstar
aznews-read-later = Read Later
aznews-not-later = Not Later
aznews-open = Open
aznews-copy-link = Copy Link
aznews-mark-all-read = Mark All Read
aznews-toolbar-add-feed = Add Feed
aznews-search = Search
aznews-search-articles = Search articles

## The source list

aznews-all-articles = All Articles
aznews-unread = Unread
aznews-starred = Starred
aznews-broken-feeds = Broken Feeds
aznews-all = All
aznews-paused = paused
aznews-menu-refresh = Refresh
aznews-mark-all-as-read = Mark All as Read
aznews-menu-edit = Edit…
aznews-follow = Follow
aznews-stop-following = Stop Following
aznews-menu-remove = Remove…
aznews-news-activity = NEWS ACTIVITY
aznews-fetching-feeds = Fetching feeds…
aznews-feeds-done = { $done } of { $total } feeds
aznews-activity-summary = { $unread } unread · { $followed } of { $count } feeds followed
aznews-add-a-feed = Add a feed
aznews-hide-activity = Hide the activity
aznews-show-activity = Show the activity
aznews-actions = Actions
aznews-manage-sources = Manage Sources…
aznews-menu-add-feed = Add Feed…
aznews-menu-import = Import Subscriptions (OPML)…
aznews-menu-export = Export Subscriptions (OPML)
aznews-menu-settings = Settings…

## Ages and dates

aznews-age-now = now
aznews-age-min = { $n } min
aznews-age-h = { $n } h
aznews-age-d = { $n } d
aznews-just-now = just now
aznews-ago = { $age } ago
aznews-never = never
aznews-not-refreshed = Not refreshed yet
aznews-updated = Updated { $when }
aznews-asked = Asked { $when }

## The article list

aznews-view-all = All articles
aznews-view-later = Read later
aznews-view-broken = Broken feeds
aznews-no-title = (no title)
aznews-reading-feeds = Reading your feeds…
aznews-no-feeds = No feeds yet
aznews-no-feeds-detail = Add a feed by its address or a website's, or import an OPML file from another reader.
aznews-add-feed = Add feed
aznews-nothing-here = Nothing here
aznews-no-articles-in-view = No articles in this view.
aznews-no-match = No article matches “{ $query }”.
aznews-count-unread =
    { $count ->
        [one] 1 article
       *[other] { $count } articles
    }, { $unread } unread
aznews-mark-all-ask =
    { $count ->
        [one] Mark the article here as read?
       *[other] Mark the { $count } articles here as read?
    }
aznews-mark-as-read = Mark as read
aznews-col-title = Title
aznews-col-source = Source
aznews-col-date = Date
aznews-sort-unread = Sort by unread
aznews-sort-starred = Sort by starred
aznews-sort-title = Sort by title
aznews-sort-source = Sort by source
aznews-sort-date = Sort by date

## The reading pane

aznews-no-article = No article selected
aznews-no-article-detail = Pick an article in the list, or add a feed.
aznews-reading-time = Reading time
aznews-minutes = { $n } min
aznews-author = Author
aznews-link = Link
aznews-pictures-not-shown =
    { $count ->
        [one] 1 picture is not shown (Settings, Reading, Pictures).
       *[other] { $count } pictures are not shown (Settings, Reading, Pictures).
    }
aznews-pictures-not-loaded =
    { $count ->
        [one] 1 picture was not loaded: the site it is on learns nothing of what you read.
       *[other] { $count } pictures were not loaded: the sites they are on learn nothing of what you read.
    }
aznews-load-pictures = Load pictures
aznews-layout-list = List
aznews-layout-magazine = Magazine
aznews-magazine-pictures-off = Pictures load when asked (Settings).
aznews-summary-only = The feed carries a summary of this article.
aznews-show-full-text = Show the full text
aznews-loading-full-text = Loading the full text from the article's page...
aznews-full-text-shown = The full text, from the article's page.
aznews-show-feed-text = Show the feed's text
aznews-no-full-text = The article's page holds no text to show here.
aznews-full-text-failed = The full text could not be loaded: { $why }
aznews-try-again = Try again
aznews-play-episode = Play { $name }
aznews-play = Play
aznews-pause = Pause
aznews-stop-episode = Stop
aznews-episode-position = Position in the episode
aznews-episode-failed = The episode does not play: { $why }
aznews-image = [image]
aznews-image-alt = [image: { $alt }]

## Add a feed

aznews-website-or-feed = Website or feed
aznews-find = Find
aznews-looking-for-feeds = Looking for feeds…
aznews-found-feeds =
    { $count ->
        [one] Found 1 feed
       *[other] Found { $count } feeds
    }
aznews-count-articles =
    { $count ->
        [one] 1 article
       *[other] { $count } articles
    }
aznews-folder = Folder
aznews-no-folder = No folder
aznews-subscribe = Subscribe
aznews-type-address = Type the address of a website or of a feed.
aznews-not-web-address = “{ $address }” is not a web address.
aznews-no-feed-on = There is no feed on { $url }.
aznews-feeds-unreadable = { $url } names feeds, but none of them could be read.
aznews-not-modified-first = The server answered “not modified” to a first request.

## Import

aznews-import-title = Import subscriptions (OPML)
aznews-import-dialog = Import subscriptions
aznews-file = File
aznews-opml-placeholder = /path/to/subscriptions.opml
aznews-choose = Choose…
aznews-read-file = Read
aznews-reading-file = Reading the file…
aznews-import-summary = { $count } feeds, { $new } new, { $known } subscribed already
aznews-row-subscribed = subscribed
aznews-row-new = new
aznews-import = Import
aznews-import-no-feed = The file lists no feed.
aznews-import-not-opml = This is not an OPML file.
aznews-file-missing = “{ $file }” does not exist.
aznews-file-unreadable = The file could not be read: { $why }
aznews-not-a-file = “{ $path }” is not a file.
aznews-type-path = Type the path of an .opml file, or choose one.
aznews-imported =
    { $count ->
        [one] 1 feed imported
       *[other] { $count } feeds imported
    }

## A feed's page and the sources page

aznews-name = Name
aznews-feed-title-placeholder = The feed's own title
aznews-address = Address
aznews-website = Website
aznews-format = Format
aznews-articles-unread = { $count } ({ $unread } unread)
aznews-followed = Followed
aznews-followed-yes = yes
aznews-followed-no = no - not refreshed, not in All articles (Sources)
aznews-last-asked = Last asked
aznews-refresh-failed = The last refresh failed: { $why }
aznews-error-http = the server answered HTTP { $status }
aznews-error-not-a-feed = this is not a feed
aznews-error-web-page = this address is a web page, not a feed
aznews-refresh-now = Refresh now
aznews-show-articles = Show its articles
aznews-unsubscribe = Unsubscribe
aznews-unsubscribe-delete = Unsubscribe and delete its articles
aznews-done = Done
aznews-sources-note = { $count } feeds, { $followed } followed. A feed you do not follow keeps its articles, but it is not refreshed and its articles stay out of All articles and Unread.
aznews-import-opml = Import OPML…
aznews-export-opml = Export OPML
aznews-row-no-folder = no folder
aznews-not-followed = not followed
aznews-remove-delete = Remove and delete its articles
aznews-feed-removed = The feed was removed.

## Settings

aznews-category-reading = Reading
aznews-category-refresh = Refresh
aznews-font-size = Font size
aznews-line-width = Line width
aznews-measure-narrow = Narrow
aznews-measure-medium = Medium
aznews-measure-wide = Wide
aznews-paper = Paper
aznews-paper-theme = Follow the theme
aznews-paper-sepia = Sepia
aznews-pictures = Pictures
aznews-pictures-always = Always
aznews-pictures-click = On click
aznews-pictures-never = Never
aznews-strip-tracking = Strip tracking
aznews-reading-note = Articles are set in a serif at the size chosen; their colours follow the app's theme and its light or dark mode. Tracking parameters (utm_*, fbclid, ...) come off links.
aznews-refresh-on-start = Refresh on start
aznews-refresh-every = Refresh every
aznews-every-never = Never
aznews-every-15-min = 15 min
aznews-every-hour = Hourly
aznews-every-4-hours = 4 hours
aznews-keep-articles = Keep articles
aznews-keep-week = A week
aznews-keep-month = A month
aznews-keep-three-months = Three months
aznews-refresh-note = A refresh asks each followed feed with what it said last time (ETag, Last-Modified): a feed without news costs nothing. Your feeds, articles and marks are files in { $folder }.

## Shortcuts

aznews-shortcut-next = Next article
aznews-shortcut-previous = Previous article
aznews-shortcut-star = Star or unstar the article
aznews-shortcut-read = Mark the article read or unread
aznews-shortcut-later = Read later
aznews-shortcut-open = Open the original in the browser
aznews-shortcut-get-news = Get news: refresh every followed feed
aznews-shortcut-import = Import an OPML file
aznews-shortcut-export = Export the subscriptions as OPML
aznews-shortcut-leave-form = Leave a form
aznews-shortcut-panes = Next / previous pane

## Notices

aznews-nothing-to-refresh = Nothing to refresh.
aznews-followed-again = { $name } is followed again
aznews-no-longer-followed = { $name } is no longer followed
aznews-files-not-read =
    { $count ->
        [one] 1 file could not be read fully - see the log
       *[other] { $count } files could not be read fully - see the log
    }
aznews-files-not-written =
    { $count ->
        [one] 1 file could not be written - see the log
       *[other] { $count } files could not be written - see the log
    }
aznews-list-not-written = The subscription list could not be written - see the log. Close the window again to quit without it.
aznews-exported-to = Exported to { $path }
aznews-marked-read =
    { $count ->
        [one] 1 article marked as read
       *[other] { $count } articles marked as read
    }
aznews-no-link = This article has no link.
aznews-link-copied = The article's link is on the clipboard.
