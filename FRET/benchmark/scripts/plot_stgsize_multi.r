#!/usr/bin/env Rscript
library(ggplot2)

# Function to plot multiple files
plot_multiple_files <- function(file_paths) {
  all_data <- data.frame()
  
  for (file_path in file_paths) {
    # Read the CSV file without headers
    data <- read.csv(file_path, header = FALSE)
    data['V5'] <- data['V5']/(3600*1000)
    
    # Extract the name for the line
    application <- sub("_.*", "", basename(file_path))
    data$application <- application
    
    # Combine data
    all_data <- rbind(all_data, data)
  }
  
  # Plot the line chart
  p <- ggplot(all_data, aes(x = V5, y = V2, color = application)) +
    geom_line() +
    labs(x = "runtime [h]", y = "# of nodes") +
    theme_minimal()
  
  # Save the plot
  ggsave("stg_node_sizes.png", plot = p + theme_bw(base_size = 10), width = 4, height = 1.5, dpi = 300, units = "in", device = "png")
}

# Example usage
file_paths <- commandArgs(trailingOnly = TRUE)
plot_multiple_files(file_paths)