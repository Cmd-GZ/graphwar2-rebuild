#!/usr/bin/env python3
"""兼容入口：真正实现在 predict_obstacles.py（docs/56）。"""
import runpy, os
runpy.run_path(os.path.join(os.path.dirname(os.path.abspath(__file__)), 'predict_obstacles.py'), run_name='__main__')
