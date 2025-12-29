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
  waters_seq_bytes=0, # via INSERT_WC
  waters_seq_int=0, # via INSERT_WC + manual interrupt
  #waters_seq_int=219542, # via INSERT_WC + manual interrupt
  waters_seq_full=0,# via INSERT_WC + manual interrupt
  waters_seq_unsync_full=0,# via INSERT_WC + manual interrupt
  polycopter_seq_dataflow_full=0, # via INSERT_WC + manual interrupt
  polycopter_seq_dataflow_int=0, # via INSERT_WC + manual interrupt
  release_seq_int=0, # via fuzzer, equals to manual interrupts; Bug: Task3 y=0
  release_seq_full=0 # via INSERT_WC + manual interrupt; Bug: Task3 y=0
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
  waters_seq_bytes=0,
  waters_seq_int=0,
  waters_seq_full=0,
  waters_seq_unsync_full=0,
  polycopter_seq_dataflow_full=0,
  polycopter_seq_dataflow_int=0,
  release_seq_int=0, 
  release_seq_full=0
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

NAME_MAP <- list(
  watersIc11_seq_full="t1 10ms",
  watersIc12_seq_full="t2 10ms",
  watersIc13_seq_full="t3 10ms",
  watersIc14_seq_full="t4 10ms",
  watersIc31_seq_full="t5 spro",
  watersIc32_seq_full="t6 2ms",
  watersIc33_seq_full="t7 50ms",
  watersIc21_seq_full="t9 100ms",
  watersIc22_seq_full="t10 10ms",
  watersIc23_seq_full="t11 2ms"
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
casenames <- dbGetQuery(con, "SELECT casename FROM combos WHERE casename LIKE 'watersIc_%' GROUP BY casename")
#casenames <- dbGetQuery(con, "SELECT casename FROM combos GROUP BY casename")
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

# BREW=RdYlGn(8)
BREW=Spectral(8)

# MY_COLORS <- c(BREW[[4]], BREW[[3]], BREW[[2]], BREW[[1]], "cyan", "pink", "gray", "orange", "black", "yellow","brown")
MY_COLORS=BREW

# draw limit
max_x <- 12
min_y <- -2500
max_y <- 2500

LEGEND_POS = "bottomright"
ISNS_PER_US = (10**3)/(2**5)

print(casenames[['casename']])

legend_names <- sapply(casenames[['casename']], function(x) NAME_MAP[[x]] %||% x)
legend_colors <- BREW
legend_styles <- c(rep("solid",10),"dotted","dashed")


h_ = 300
w_ = h_*4/3

png(file=sprintf("%s/all_tasks.png", args[2]), width=w_, height=h_)
#tikz(file=sprintf("%s/all_tasks.tex", args[2]), width=0.6*w_/72, height=0.6*h_/72)
#pdf(file=sprintf("%s/all_tasks.pdf", args[2]), width=w_/72, height=h_/72)


# plot setup
par(mar=c(4,4,1,1))
par(oma=c(0,0,0,0))

plot(c(0,max_x),c(min_y,max_y), col='white', xlab="Time [h]", ylab="FRET's improvement over competitors [µs]", pch='.')

draw_plot <- function(data, casename, color) {
  # evo, cov, random, fret



  # Pre-calculate all malines and medlines
  malines_list <- list()
  medlines_list <- list()
  for (n in seq_along(data)) {
    d <- data[[n]]
    malines_list[[names(data)[n]]] <- ml2lines(d[c('max','timestamp')])
    medlines_list[[names(data)[n]]] <- ml2lines(d[c('median','timestamp')])
  }

  # Plot the difference between malines['stgwoet'] (FRET) and malines['random']
  if ("stgwoet" %in% names(malines_list) && "feedgeneration100" %in% names(malines_list)) {
    fret_malines <- malines_list[["stgwoet"]]
    compare_malines1 <- malines_list[["feedgeneration100"]]
    compare_malines2 <- malines_list[["frafl"]]
    fret_medlines <- medlines_list[["stgwoet"]]
    compare_medlines1 <- medlines_list[["feedgeneration100"]]
    compare_medlines2 <- medlines_list[["frafl"]]

    # Ensure all have the same number of rows and matching X
    min_len <- min(nrow(fret_malines), nrow(compare_malines1), nrow(compare_malines2))
    # For each point, take the max of the two compare malines
    compare_max_Y <- pmax(compare_malines1[1:min_len, "Y"], compare_malines2[1:min_len, "Y"])
    diff_lines_ma <- data.frame(
      X = fret_malines[1:min_len, "X"],
      Y = fret_malines[1:min_len, "Y"] - compare_max_Y
    )
    lines(diff_lines_ma, col=color, lty="solid", lwd=2)

    # Same for medlines
    compare_max_med_Y <- pmax(compare_medlines1[1:min_len, "Y"], compare_medlines2[1:min_len, "Y"])
    diff_lines_med <- data.frame(
      X = fret_medlines[1:min_len, "X"],
      Y = fret_medlines[1:min_len, "Y"] - compare_max_med_Y
    )
    lines(diff_lines_med, col=color, lty="dashed", lwd=2)
  }
}


for (i in seq_len(length(casenames[['casename']]))) {
  cn =casenames[['casename']][i]
  color = MY_COLORS[i]
  tables <- dbGetQuery(con, sprintf("SELECT * FROM combos WHERE casename == '%s'", cn[[1]]))
  table_list <- list()
  for (row in 1:nrow(tables)) {
    table_name <- tables[row, 'fullname']
    tool_name <- tables[row, 'toolname']
    table_data <- dbGetQuery(con, sprintf("SELECT * FROM '%s'", table_name))
    table_list[[tool_name]] <- table_data
  }
  # Convert timestamp from microseconds to hours
  for (n in seq_len(length(table_list))) {
    table_list[[n]]$timestamp <- table_list[[n]]$timestamp / 3600000
    table_list[[n]]$min <- table_list[[n]]$min / ISNS_PER_US
    table_list[[n]]$max <- table_list[[n]]$max / ISNS_PER_US
    table_list[[n]]$median <- table_list[[n]]$median / ISNS_PER_US
    table_list[[n]]$mean <- table_list[[n]]$mean / ISNS_PER_US
    table_list[[n]]$sdiv <- table_list[[n]]$sdiv / ISNS_PER_US
  }

  table_list <- table_list[c('stgwoet', 'feedgeneration100', 'frafl', 'random')] # manual re-order
  table_list <- table_list[!sapply(table_list, is.null)] # remove NULL entries
  draw_plot(table_list, cn[[1]], color)
}
legend(LEGEND_POS, legend=legend_names,#"bottomright",
      col=legend_colors,
      lty=legend_styles,
      lwd=2, ncol=2)

par(las = 2, mar = c(10, 5, 1, 1)) 

# png
## normal
dev.off()

dbDisconnect(con)
