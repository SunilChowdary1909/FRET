#!/usr/bin/env Rscript
# Load necessary libraries
library(ggplot2)

# Define the function to load CSV and plot
plot_stgsize <- function(file_path) {
  print(file_path)
  # Read the CSV file without headers
  data <- read.csv(file_path, header = FALSE)
  data['V5'] <- data['V5']/(3600*1000)

  # Plot the line chart
  p <- ggplot(data, aes(x = V5, y = V2)) +
    geom_line() +
    labs(x = "runtime [h]", y = "# of nodes") + #, title = "Number of nodes over time.") +
    theme_minimal()
    
  output_file <- sub("\\.stgsize$", paste0("_nodes.png"), file_path)
  ggsave(basename(output_file), plot = p + theme_bw(base_size = 10), width = 3.5, height = 2, dpi = 300, units = "in", device = "png")
}

args <- commandArgs(trailingOnly = TRUE)
plot_stgsize(args[1])