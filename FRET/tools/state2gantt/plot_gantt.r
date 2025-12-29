#!/usr/bin/env Rscript
# Load necessary libraries
#install.packages(c(ggplot2,readr,dplyr,plotly))
library(ggplot2)
library(readr)
library(dplyr)
library(plotly)

QEMU_SHIFT<-5
TIMESCALE<-1000000

# Function to create a Gantt chart with dots on short segments
create_gantt_chart <- function(csv_file_a, csv_file_b, MIN_WIDTH, output_format = NULL, startpoint, endpoint) {
  # Read the CSV file
  df <- read_csv(csv_file_a)
  # df_b <- read_csv(csv_file_b)
  df_b <- read_csv(csv_file_b, col_types = cols(.default = "d", name = col_character()))
  # df <- df %>% bind_rows(df_b)

  # Cut out everything outside the range
  df <- df %>%
    filter(end >= startpoint & start <= endpoint) %>% rowwise %>% mutate(end = min(end, endpoint), start = max(start, startpoint))

  df_b <- df_b %>%
    filter(end >= startpoint & start <= endpoint) %>% rowwise %>% mutate(end = min(end, endpoint), start = max(start, startpoint))

  # Add a placeholder for all tasks that don't have job instances in the range
  s <- min(df$start)
  placeholder <- df_b %>% mutate(start = s, end = s)
  df <- df %>% bind_rows(placeholder)
  
  
  # Ensure start and end columns are treated as integers
  df <- df %>%
    mutate(start = (as.integer(start) * 2**QEMU_SHIFT)/TIMESCALE,
           end = (as.integer(end) * 2**QEMU_SHIFT)/TIMESCALE)

  df_b <- df_b %>%
    mutate(start = (as.integer(start) * 2**QEMU_SHIFT)/TIMESCALE,
           end = (as.integer(end) * 2**QEMU_SHIFT)/TIMESCALE)
  
  # Calculate the segment width
  df <- df %>%
    mutate(width = end - start)
  
  # Sort the DataFrame by 'prio' column in descending order
  df <- df %>%
    arrange(prio)

  # Add labels to segments
  df$label <- paste(
    "Start:", df$start,
    "<br>",
    "Prio:", df$prio,
    "<br>",
    "Name:", df$name,
    "<br>",
    "Id:", df$state_id,
    "<br>",
    "State:", df$state,
    "<br>",
    "ABB:", df$abb,
    "<br>",
    "End:", df$end
  )
  df_b$label <- paste(
    "Start:", df_b$start,
    "<br>",
    "End:", df_b$end
  )
  
  # Create the Gantt chart with ggplot2
  p <- ggplot(df, aes(x = start, xend = end, y = reorder(name, prio), yend = name, text = label)) +
    geom_segment(aes(color = factor(prio)), size = 6) +
    labs(title = "Gantt Chart", x = "Time Step", y = "Task", color = "Priority") +
    theme_minimal()

  # Plot Ranges
  p <- p + geom_segment(data = df_b, aes(color = factor(prio)), size = 1)

  p <- p + geom_point(data = df_b, 
                      aes(x = end, y = name), 
                      color = "blue", size = 2)

  # Add dots on segments shorter than MIN_WIDTH
  p <- p + geom_point(data = df %>% filter(width < MIN_WIDTH & width > 0), 
                      aes(x = start, y = name), 
                      color = "red", size = 1)
  
  # Handle output format
  if (!is.null(output_format)) {
    output_file <- sub("\\.csv$", paste0(".", output_format), csv_file_a)
    if (output_format == "html") {
      # Convert the ggplot object to a plotly object for interactivity
      p_interactive <- ggplotly(p)
      htmlwidgets::saveWidget(p_interactive, output_file)
    } else if (output_format == "png") {
      ggsave(output_file, plot = p, device = "png")
    } else {
      stop("Invalid output format. Use 'html' or 'png'.")
    }
  } else {
    # Convert the ggplot object to a plotly object for interactivity
    p_interactive <- ggplotly(p)
    # Print the interactive Gantt chart
    print(p_interactive)
  }
}

# Main execution
args <- commandArgs(trailingOnly = TRUE)
if (length(args) < 2 || length(args) > 5) {
  stop("Usage: Rscript script.R <csv_file> <csv_file> [output_format] [<strt> <end>]")
} else {
  csv_file_a <- args[1]
  csv_file_b <- args[2]
  if (length(args) >= 3) {
    output_format <- args[3]
  } else {
    output_format <- NULL
  }
  if (length(args) >= 5) {
    start <- as.integer(args[4])
    end <- as.integer(args[5])
  } else {
    start <- 0
    end <- Inf
  }
}

MIN_WIDTH <- 500 # You can set your desired minimum width here
create_gantt_chart(csv_file_a, csv_file_b, MIN_WIDTH, output_format, start, end)
