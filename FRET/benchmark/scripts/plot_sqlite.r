# install.packages(c("mosaic", "dplyr", "DBI", "tikzDevice", "colorspace", "heatmaply", "RColorBrewer", "RSQLite"))
library("mosaic")
library("dplyr")
library("DBI")
library("tikzDevice") # Add this line to include the tikzDevice library
library("colorspace") 
library("heatmaply")
library("RColorBrewer")

args = commandArgs(trailingOnly=TRUE)

TOOL_TRANSLATION <- list(
  feedgeneration100 = "evolution",
  frafl = "coverage",
  random = "random",
  stgwoet = "FRET"
)


KNOWN_WCRT <- list(
  waters_seq_bytes=212252, # via INSERT_WC
  waters_seq_int=0, # via INSERT_WC + manual interrupt
  #waters_seq_int=219542, # via INSERT_WC + manual interrupt
  waters_seq_full=219542,# via INSERT_WC + manual interrupt
  waters_seq_unsync_full=234439,# via INSERT_WC + manual interrupt
  polycopter_seq_dataflow_full=174866, # via INSERT_WC + manual interrupt
  polycopter_seq_dataflow_int=174866, # via INSERT_WC + manual interrupt
  release_seq_int=582699, # via fuzzer, equals to manual interrupts; Bug: Task3 y=0
  release_seq_full=614583 # via INSERT_WC + manual interrupt; Bug: Task3 y=0
  )

STATIC_WCRT <- list(
  waters_seq_bytes=256632,
  waters_seq_int=256632,
  waters_seq_full=256632,
  waters_seq_unsync_full=272091,
  polycopter_seq_dataflow_full=373628,
  polycopter_seq_dataflow_int=373628,
  release_seq_int=921360, 
  release_seq_full=921360
  )

# ISNS_PER_US = (10**3)/(2**5)
# print(list(sapply(STATIC_WCRT, function(x) x/ISNS_PER_US)))
# quit()

STATIC_WCRT <- list(
  waters_seq_bytes=0,
  waters_seq_int=0,
  waters_seq_full=0,
  waters_seq_unsync_full=0,
  polycopter_seq_dataflow_full=0,
  polycopter_seq_dataflow_int=0,
  release_seq_int=0, 
  release_seq_full=0
  )

MIN_Y <- list(
  waters_seq_bytes=5250,
  waters_seq_int=5700,
  waters_seq_full=5250,
  waters_seq_unsync_full=0,
  polycopter_seq_dataflow_full=0,
  polycopter_seq_dataflow_int=0,
  release_seq_int=16500, 
  release_seq_full=16500
  )

LEG_POS <- list(
  waters_seq_bytes="bottomright",
  waters_seq_int="bottomright",
  waters_seq_full="bottomright",
  waters_seq_unsync_full="bottomright",
  polycopter_seq_dataflow_full="bottomright",
  polycopter_seq_dataflow_int="bottomright",
  release_seq_int="bottomright", 
  release_seq_full="bottomright"
  )

# Read the first command line argument as an sqlite file
if (length(args) > 0) {
  sqlite_file <- args[1]
  con <- dbConnect(RSQLite::SQLite(), sqlite_file)
} else {
  print("No sqlite file provided, assume defaults")
  args = c("bench.sqlite", "remote")
  sqlite_file <- args[1]
  con <- dbConnect(RSQLite::SQLite(), sqlite_file)
}

combos <- dbGetQuery(con, "SELECT * FROM combos")
casenames <- dbGetQuery(con, "SELECT casename FROM combos WHERE NOT casename LIKE 'watersIc_%' GROUP BY casename")
# casenames <- dbGetQuery(con, "SELECT casename FROM combos GROUP BY casename")
toolnames <- dbGetQuery(con, "SELECT toolname FROM combos GROUP BY toolname")

ml2lines <- function(ml, casename) {
  lines = NULL
  last = 0
  for (i in seq_len(dim(ml)[1])) {
    lines = rbind(lines, cbind(X=last, Y=ml[i,1]))
    lines = rbind(lines, cbind(X=ml[i,2], Y=ml[i,1]))
    last = ml[i,2]
  }
  return(lines)
}

BREW=RdYlGn(4)
# BREW=Spectral(4)

draw_plot <- function(data, casename) {
  # evo, cov, random, fret
  MY_COLORS <- c(BREW[[4]], BREW[[3]], BREW[[2]], BREW[[1]], "cyan", "pink", "gray", "orange", "black", "yellow","brown")
  # MY_COLORS <- c("orange", "blue", "red", "green", "orange", "cyan", "pink", "gray", "orange", "black", "yellow","brown")
  # MY_COLORS <- c("green", "blue", "red", "magenta", "orange", "cyan", "pink", "gray", "orange", "black", "yellow","brown")
  LEGEND_POS=LEG_POS[[casename]]
  if (is.null(LEGEND_POS)) {
    LEGEND_POS = "bottomright"
  }

  ISNS_PER_US = (10**3)/(2**5)

  # Convert timestamp from microseconds to hours
  for (n in seq_len(length(data))) {
    data[[n]]$timestamp <- data[[n]]$timestamp / 3600000
    data[[n]]$min <- data[[n]]$min / ISNS_PER_US
    data[[n]]$max <- data[[n]]$max / ISNS_PER_US
    data[[n]]$median <- data[[n]]$median / ISNS_PER_US
    data[[n]]$mean <- data[[n]]$mean / ISNS_PER_US
    data[[n]]$sdiv <- data[[n]]$sdiv / ISNS_PER_US
  }

  data <- data[c('stgwoet', 'feedgeneration100', 'frafl', 'random')] # manual re-order
  data <- data[!sapply(data, is.null)] # remove NULL entries

  wcrt = KNOWN_WCRT[[casename]]
  if (!is.null(wcrt)) {
    wcrt = wcrt / ISNS_PER_US
  } else {
    wcrt = 0
  }
  static_wcrt = STATIC_WCRT[[casename]]
  if (!is.null(static_wcrt)) {
    static_wcrt = static_wcrt / ISNS_PER_US
  } else {
    static_wcrt = 0
  }

  # draw limits
  max_x <- max(sapply(data, function(tbl) max(tbl$timestamp, na.rm = TRUE)))
  max_x <- min(max_x, 24) # quick fix, cap to 16h
  max_y <- max(wcrt,max(sapply(data, function(tbl) max(tbl$max, na.rm = TRUE))))
  min_y <- min(sapply(data, function(tbl) min(tbl$min, na.rm = TRUE)))
  min_y <- max(min_y, MIN_Y[[casename]])

  # draw static wcrt
  max_y <- max(max_y, static_wcrt)

  # plot setup
  par(mar=c(4,4,1,1))
  par(oma=c(0,0,0,0))

  plot(c(0,max_x),c(min_y,max_y), col='white', xlab="Time [h]", ylab="WORT [µs]", pch='.')

  # plot data
  for (n in seq_len(length(data))) {
    d <- data[[n]]
    malines = ml2lines(d[c('max','timestamp')])
    lines(malines, col=MY_COLORS[[n]], lty='solid', lwd=2) # Increase line width
    medlines = ml2lines(d[c('median','timestamp')])
    lines(medlines, col=MY_COLORS[[n]], lty='dashed', lwd=2) # Increase line width
    # milines = ml2lines(d[c('min','timestamp')])
    # lines(milines, col=MY_COLORS[[n]], lty='dashed', lwd=2) # Increase line width
  }

  legend_names <- sapply(names(data), function(n) TOOL_TRANSLATION[[n]])
  legend_colors <- c(MY_COLORS[1:length(data)],"grey","grey")
  legend_styles <- c(rep("solid",length(data)),"dotted","dashed")

  if (wcrt > 0) {
    # abline(h=wcrt, col='grey', lty='dotted', lwd=3)
    abline(h=max(wcrt,max(sapply(data, function(tbl) max(tbl$max, na.rm = TRUE)))), col='grey', lty='dotted', lwd=3) # If the manual WCRT was slightly too low
    legend_names <- c(legend_names, "WCRT")
  }
  if (static_wcrt > 0) {
    abline(h=static_wcrt, col='grey', lty='dashed', lwd=3)
    legend_names <- c(legend_names, "static bound")
  }

  legend(LEGEND_POS, legend=legend_names,#"bottomright",
        col=legend_colors,
        lty=legend_styles,
        lwd=2)

  par(las = 2, mar = c(10, 5, 1, 1)) 
}

print(casenames[['casename']])
for (cn in casenames[['casename']]) {
  tables <- dbGetQuery(con, sprintf("SELECT * FROM combos WHERE casename == '%s'", cn[[1]]))
  table_list <- list()
  for (row in 1:nrow(tables)) {
    table_name <- tables[row, 'fullname']
    tool_name <- tables[row, 'toolname']
    table_data <- dbGetQuery(con, sprintf("SELECT * FROM '%s'", table_name))
    table_list[[tool_name]] <- table_data
  }
  h_ = 300
  w_ = h_*4/3
  # png
  ## normal
  png(file=sprintf("%s/sql_%s.png", args[2],cn[[1]]), width=w_, height=h_)
  draw_plot(table_list, cn[[1]])
  dev.off()
  ## wide
  png(file=sprintf("%s/sql_%s_wide.png", args[2],cn[[1]]), width=2*w_, height=h_)
  draw_plot(table_list, cn[[1]])
  dev.off()
  # tikz
  ## normal
  tikz(file=sprintf("%s/sql_%s.tex", args[2],cn[[1]]), width=0.6*w_/72, height=0.6*h_/72)
  draw_plot(table_list, cn[[1]])
  dev.off()
  ## wide
  tikz(file=sprintf("%s/sql_%s_wide.tex", args[2],cn[[1]]), width=(w_*2)/72, height=h_/72)
  draw_plot(table_list, cn[[1]])
  dev.off()
  # pdf
  ## normal
  pdf(file=sprintf("%s/sql_%s.pdf", args[2],cn[[1]]), width=w_/72, height=h_/72)
  draw_plot(table_list, cn[[1]])
  dev.off()
  ## wide
  pdf(file=sprintf("%s/sql_%s_wide.pdf", args[2],cn[[1]]), width=2*w_/72, height=h_/72)
  draw_plot(table_list, cn[[1]])
  dev.off()
}

dbDisconnect(con)
